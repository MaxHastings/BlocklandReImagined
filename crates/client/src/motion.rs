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
/// The fastest the driven vehicle's correction eases out, units and
/// radians per second: the chase camera rides the drawn vehicle, so a
/// large correction glides out instead of whipping the whole view.
const DRIVE_EASE_SPEED: f32 = 4.0;
const DRIVE_EASE_TURN: f32 = 1.0;
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
    remotes: BTreeMap<OwnerId, VecDeque<bri_net::protocol::Pose>>,
    /// Estimated `server_tick - local_seconds * TICK_RATE`.
    clock_offset: Option<f64>,
    /// The offset presented, slewing toward `clock_offset`.
    shown_offset: Option<f64>,
    local_seconds: f64,
    newest_local_tick: u64,
    presented: BTreeMap<OwnerId, PlayerState>,
    /// Each player's latest simulated tick, uninterpolated.
    ticked: BTreeMap<OwnerId, PlayerState>,
    /// The server tick of each remote's `ticked` pose.
    ticked_at: BTreeMap<OwnerId, u64>,
    local_eye: Option<Vec3>,
    mounted: bool,
    /// Last input sequence sent before a map change reset prediction.
    sent_sequence: u64,
    /// Fastest speed into a surface since `take_impact` (`Player::updatePos`
    /// `bd`), for the ground impact camera shake.
    impact: f32,
    /// The vehicle this client drives and predicts.
    driving: Option<u64>,
    /// The driven vehicle's visual correction after a replay, blended out
    /// like the body's.
    drive_offset: Vec3,
    drive_turn: glam::Quat,
    /// Openings the local body went through since `take_passed`, their
    /// carries composed: the look turns by it.
    passed: Option<glam::Affine3A>,
    /// An opening the prediction went through that the drawn body has not
    /// reached yet (it is drawn up to a Torque tick behind).
    unshown: Option<Unshown>,
}

/// The prediction moves the body through an opening on the tick its middle
/// crosses, but draws it between its last two ticks, a little behind. Until
/// the drawn middle crosses too, the body, the look and the camera stay on
/// the near side, so the picture never changes at the crossing: what the
/// near side showed through the opening is what the far side shows.
struct Unshown {
    carry: glam::Affine3A,
    /// The opening gone in through, on the near side.
    entry: bri_content::passage::Passage,
    /// Seconds since the crossing; the view goes through regardless after
    /// [`UNSHOWN_SECONDS`].
    age: f32,
}
/// Longest the view stays behind a crossing (about two Torque ticks past
/// the farthest the drawn body lags).
const UNSHOWN_SECONDS: f32 = 0.1;

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
    /// The vehicle this client is predicting, if any.
    pub fn driving(&self) -> Option<u64> {
        self.driving
    }
    /// Start predicting the vehicle this client drives (`id`, the vehicle
    /// pack, its spawn and its newest replicated motion), or stop. Needs the
    /// body's predictor; without one nothing is predicted.
    pub fn drive(
        &mut self,
        vehicle: Option<(
            u64,
            bri_vehicles::Pack,
            bri_sim::prediction::DriveSpawn,
            bri_vehicles::Motion,
        )>,
    ) -> Result<()> {
        let Some(predictor) = &mut self.predictor else {
            self.driving = None;
            return Ok(());
        };
        self.drive_offset = Vec3::ZERO;
        self.drive_turn = glam::Quat::IDENTITY;
        self.driving = None;
        match vehicle {
            Some((id, pack, spawn, motion)) => {
                predictor.drive(Some((pack, spawn, motion)))?;
                self.driving = Some(id);
            }
            None => predictor.drive(None)?,
        }
        Ok(())
    }
    pub fn set_drive_prefs(&mut self, prefs: (bool, bool)) {
        if let Some(predictor) = &mut self.predictor {
            predictor.set_drive_prefs(prefs);
        }
    }
    /// Correct the driven vehicle from a newer host pose, carrying the jump
    /// in its drawn place as a correction that fades.
    pub fn observe_vehicle(&mut self, pose: &bri_sim::session::VehiclePose) -> Result<()> {
        if self.driving != Some(pose.id) {
            return Ok(());
        }
        let Some((_, before, before_rotation)) = self.drawn_drive() else {
            return Ok(());
        };
        let Some(predictor) = &mut self.predictor else {
            return Ok(());
        };
        if predictor
            .drive_pose(pose.tick, pose.driver_input, &pose.motion())?
            .is_none()
        {
            return Ok(());
        }
        // The replay moves both ticks the drawn place blends between, so
        // the whole drawn difference is carried, not just the newest tick's.
        let Some((_, now, now_rotation)) = self.drawn_drive() else {
            return Ok(());
        };
        let offset = before - now;
        let turn = before_rotation * now_rotation.inverse();
        self.drive_offset += offset;
        self.drive_turn = (self.drive_turn * turn).normalize();
        if !self.drive_offset.is_finite()
            || self.drive_offset.length() > SNAP_DISTANCE
            || !self.drive_turn.is_finite()
        {
            self.drive_offset = Vec3::ZERO;
            self.drive_turn = glam::Quat::IDENTITY;
        }
        Ok(())
    }
    /// Where the driven vehicle is drawn: between its last two predicted
    /// ticks, with any correction still fading.
    pub fn driven_frame(&self) -> Option<(u64, Vec3, glam::Quat)> {
        let (id, position, rotation) = self.drawn_drive()?;
        Some((
            id,
            position + self.drive_offset,
            (self.drive_turn * rotation).normalize(),
        ))
    }
    /// The predicted place between the driven vehicle's last two ticks,
    /// without the correction.
    fn drawn_drive(&self) -> Option<(u64, Vec3, glam::Quat)> {
        let (id, previous, current) = self.predictor.as_ref()?.driven()?;
        let alpha = (self.accumulator / TICK).clamp(0.0, 1.0);
        let position = Vec3::from(previous.position).lerp(Vec3::from(current.position), alpha);
        let rotation = glam::Quat::from_array(previous.rotation)
            .normalize()
            .slerp(glam::Quat::from_array(current.rotation).normalize(), alpha);
        Some((id, position, rotation.normalize()))
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
            // A stand-in until the rider's body is posed; the app then sees
            // from its `eye` node (`App::rider_eye`).
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
        let fade = (-CORRECTION_RATE * seconds).exp();
        self.correction *= fade;
        if self.correction.length_squared() < 1e-8 {
            self.correction = Vec3::ZERO;
        }
        let length = self.drive_offset.length();
        if length > 0.0 {
            let keep = fade.max(1.0 - DRIVE_EASE_SPEED * seconds / length);
            self.drive_offset *= keep;
        }
        let angle = self.drive_turn.angle_between(glam::Quat::IDENTITY);
        if angle > 0.0 {
            let keep = fade.max(1.0 - DRIVE_EASE_TURN * seconds / angle);
            self.drive_turn = glam::Quat::IDENTITY.slerp(self.drive_turn, keep).normalize();
        }
        let Some(predictor) = &mut self.predictor else {
            return Ok(None);
        };
        self.accumulator += seconds;
        let mut steps = 0;
        if let Some(unshown) = &mut self.unshown {
            unshown.age += seconds;
        }
        while self.accumulator >= TICK && steps < MAX_STEPS {
            self.accumulator -= TICK;
            // The look is still the near side's while the view is: the body
            // on the far side moves and looks as it turned.
            let input = match &self.unshown {
                Some(unshown) => carried_input(input, &unshown.carry),
                None => input,
            };
            if self.mounted {
                predictor.record(input)?;
            } else {
                let before = predictor.state().clone();
                let (_, events) = predictor.step(input)?;
                for (_, speed) in events.hits {
                    self.impact = self.impact.max(speed);
                }
                let middle = predictor.player_middle();
                let lift = glam::Vec3::Y * middle;
                let from = Vec3::from(before.feet) + lift;
                self.previous = Some(match events.passed {
                    Some(carry) => before.carried(&carry, middle),
                    None => before,
                });
                if let Some(carry) = events.passed {
                    // Through an opening: drawn from the far side between
                    // ticks, shown from the near side until it gets there.
                    if let Some(unshown) = self.unshown.take() {
                        self.passed =
                            Some(unshown.carry * self.passed.unwrap_or(glam::Affine3A::IDENTITY));
                    }
                    let to = carry
                        .inverse()
                        .transform_point3(Vec3::from(predictor.state().feet) + lift);
                    match predictor.world().links().passages().first(from, to) {
                        Some((entry, _)) => {
                            self.unshown = Some(Unshown {
                                carry,
                                entry: *entry,
                                age: 0.0,
                            })
                        }
                        None => {
                            self.passed =
                                Some(carry * self.passed.unwrap_or(glam::Affine3A::IDENTITY))
                        }
                    }
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
        // Every input this frame produced plus recent history: a slow frame
        // that ran more ticks than the redundancy window loses none of them.
        let sent = (steps > 0).then(|| {
            let recent: Vec<_> = predictor
                .recent(redundancy.max(steps as usize))
                .map(|(_, i)| *i)
                .collect();
            (predictor.sequence(), recent)
        });
        self.show_crossing();
        Ok(sent)
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
        self.ticked_at.clear();
        self.local_eye = None;
        if !self.present_local(view.owner, yaw, pitch, head_yaw)
            && let Some(pose) = view.poses.get(&view.owner)
        {
            self.presented.insert(view.owner, pose.player.clone());
        }
        let render_tick = self.server_tick().map(|tick| tick - INTERPOLATION_TICKS);
        let passages = self
            .collision()
            .map(|c| c.links().passages().clone())
            .unwrap_or_default();
        for (owner, pose) in &view.poses {
            if *owner == view.owner {
                continue;
            }
            let (state, ticked) = match (self.remotes.get(owner), render_tick) {
                (Some(history), Some(tick)) if !history.is_empty() => (
                    sample(history, tick, &passages),
                    history
                        .iter()
                        .take_while(|pose| pose.tick as f64 <= tick)
                        .last()
                        .unwrap_or(history.front().unwrap()),
                ),
                _ => (pose.player.clone(), pose),
            };
            self.ticked_at.insert(*owner, ticked.tick);
            self.ticked.insert(*owner, ticked.player.clone());
            self.presented.insert(*owner, state);
        }
        &self.presented
    }
    /// The local player's presented state from its prediction, looking
    /// (`yaw`, `pitch`, `head_yaw`) as the controls do this frame. False
    /// while nothing is predicted.
    fn present_local(&mut self, owner: OwnerId, yaw: f32, pitch: f32, head_yaw: f32) -> bool {
        let Some(predictor) = &self.predictor else {
            return false;
        };
        let current = predictor.state();
        self.ticked.insert(owner, current.clone());
        let mut state = self.drawn(predictor);
        let feet = Vec3::from(state.feet);
        state.yaw = yaw;
        state.pitch = pitch;
        state.head_yaw = head_yaw;
        let tuning = predictor.tuning().clone();
        let height = self
            .eye_height
            .unwrap_or_else(|| state.eye(&tuning).y - feet.y);
        let ahead = Vec3::new(yaw.sin(), 0.0, -yaw.cos()) * tuning.eye_forward;
        self.local_eye = Some(feet + Vec3::Y * height + ahead);
        self.presented.insert(owner, state);
        true
    }
    /// The predicted body as drawn this frame: between its last two ticks,
    /// eased by any correction, and on the near side of an opening the
    /// prediction went through but the drawn body has not reached yet.
    fn drawn(&self, predictor: &Predictor) -> PlayerState {
        let current = predictor.state();
        let previous = self.previous.as_ref().unwrap_or(current);
        let alpha = (self.accumulator / TICK).clamp(0.0, 1.0);
        let mut state = blend(previous, current, alpha);
        state.feet = (Vec3::from(state.feet) + self.correction).to_array();
        match &self.unshown {
            Some(unshown) => state.carried(&unshown.carry.inverse(), predictor.player_middle()),
            None => state,
        }
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
    /// The server tick of the remote pose `ticked` returns.
    pub fn ticked_at(&self, owner: OwnerId) -> Option<u64> {
        self.ticked_at.get(&owner).copied()
    }
    /// The openings bodies pass through, as the local copy of the world
    /// has them.
    pub fn passages(&self) -> bri_content::passage::Passages {
        self.collision()
            .map(|c| c.links().passages().clone())
            .unwrap_or_default()
    }
    /// The turn and carry of openings the local body went through since
    /// last asked: the caller turns the player's look by the turn.
    pub fn take_passed(&mut self) -> Option<glam::Affine3A> {
        self.passed.take()
    }
    /// Let the view through an opening once the drawn body's middle has
    /// crossed it (or it has waited long enough): the look turns this
    /// frame (`take_passed`) and the frame draws from the far side.
    fn show_crossing(&mut self) {
        let (Some(unshown), Some(predictor)) = (&self.unshown, &self.predictor) else {
            return;
        };
        let middle = Vec3::from(self.drawn(predictor).feet) + Vec3::Y * predictor.player_middle();
        if unshown.entry.side(middle) > 0.0 && unshown.age < UNSHOWN_SECONDS && !self.mounted {
            return;
        }
        let carry = unshown.carry;
        self.unshown = None;
        self.passed = Some(carry * self.passed.unwrap_or(glam::Affine3A::IDENTITY));
    }
    /// Smoothed local eye (prediction, render interpolation and crouch blend).
    pub fn local_eye(&self) -> Option<Vec3> {
        self.local_eye
    }
}

fn sample(
    history: &VecDeque<bri_net::protocol::Pose>,
    tick: f64,
    passages: &bri_content::passage::Passages,
) -> PlayerState {
    let first = history.front().unwrap();
    if tick <= first.tick as f64 {
        let mut state = first.player.clone();
        state.feet = state.shown_feet();
        return state;
    }
    for (a, b) in history.iter().zip(history.iter().skip(1)) {
        if tick <= b.tick as f64 {
            let span = (b.tick - a.tick).max(1) as f64;
            return blend_through(
                &a.player,
                &b.player,
                ((tick - a.tick as f64) / span) as f32,
                passages,
            );
        }
    }
    let last = history.back().unwrap();
    let ahead = ((tick - last.tick as f64).min(EXTRAPOLATION_TICKS) / TICK_RATE) as f32;
    let mut state = last.player.clone();
    let feet = Vec3::from(state.shown_feet()) + Vec3::from(state.velocity) * ahead;
    state.feet = feet.to_array();
    state
}

/// The input of a look turned by an opening's carry: the same direction on
/// the far side.
fn carried_input(input: MoveInput, carry: &glam::Affine3A) -> MoveInput {
    let (yaw, pitch, _) = crate::portal_view::carried_look((input.yaw, input.pitch, 0.0), carry);
    MoveInput {
        yaw,
        pitch: pitch.clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2),
        ..input
    }
}
/// [`blend`] of two poses a body may have gone through an opening
/// between: drawn moving on from the far side, never sliding across.
fn blend_through(
    a: &PlayerState,
    b: &PlayerState,
    t: f32,
    passages: &bri_content::passage::Passages,
) -> PlayerState {
    let middle = bri_sim::player::nominal_middle(a.scale);
    let lift = Vec3::Y * middle;
    let carry = (!passages.is_empty())
        .then(|| {
            passages.bridge(
                Vec3::from(a.shown_feet()) + lift,
                Vec3::from(b.shown_feet()) + lift,
            )
        })
        .flatten();
    match carry {
        Some(carry) => blend(&a.carried(&carry, middle), b, t),
        None => blend(a, b, t),
    }
}
/// The angle `t` of the way from `a` to `b` (radians), turning the short way
/// round and wrapped to [-pi, pi). Every replicated heading or yaw drawn
/// between two snapshots blends through this: a plain lerp from 3.1 to -3.1
/// sweeps through 0, a full half turn the wrong way for one frame.
pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let turn = (b - a + PI).rem_euclid(2.0 * PI) - PI;
    (a + turn * t + PI).rem_euclid(2.0 * PI) - PI
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
    out.yaw = lerp_angle(a.yaw, b.yaw, t);
    out.pitch = a.pitch + (b.pitch - a.pitch) * t;
    out.head_yaw = a.head_yaw + (b.head_yaw - a.head_yaw) * t;
    out
}

#[cfg(test)]
mod crossing_tests;
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
            tick: Default::default(),
        }
    }
    fn pose(tick: u64, x: f32, yaw: f32) -> bri_net::protocol::Pose {
        bri_net::protocol::Pose {
            tick,
            acknowledged_input: 0,
            spawn_tick: 0,
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
        let history: VecDeque<_> = [pose(3, 0.0, 3.0), pose(6, 3.0, -3.0)].into();
        assert_eq!(sample(&history, 0.0, &Default::default()).feet[0], 0.0);
        assert!((sample(&history, 4.5, &Default::default()).feet[0] - 1.5).abs() < 1e-5);
        // Shortest arc through +/-PI rather than spinning through zero.
        assert!(sample(&history, 4.5, &Default::default()).yaw.abs() > 3.0);
        // Extrapolation is bounded to EXTRAPOLATION_TICKS of velocity.
        let far = sample(&history, 1000.0, &Default::default()).feet[0];
        assert!((far - (3.0 + 10.0 * 6.0 / 120.0)).abs() < 1e-4);
    }
    /// How the driven vehicle is drawn over a real connection: the host
    /// runs the moves as they arrive (late, jittered, sometimes starved),
    /// its poses come back jittered, and the client draws uneven frames.
    struct DriveRun {
        /// The largest jump of the drawn pose when a host pose is applied
        /// (units, radians): the correction must never show as a pop.
        pop: (f32, f32),
        /// The fastest the fading correction alone moves the drawn pose
        /// (units/s, rad/s): the whip a rigid chase camera shows.
        whip: (f32, f32),
        /// Corrections the host's poses made past the first two seconds
        /// (moving the drawn place over 1 cm or 0.01 rad), and the largest.
        corrections: usize,
        worst: (f32, f32),
        /// The largest frame-to-frame change of the drawn pose's velocity
        /// and spin (units/s, rad/s): a stair-stepping pose shows here.
        jerk: (f32, f32),
        /// Frames whose drawn velocity changed by more than 8 units/s: a
        /// jump's take-off or landing is one or two, a stair-step most.
        rough: usize,
    }
    fn vehicle_pack() -> Option<bri_vehicles::Pack> {
        pack_for("")
    }
    /// The pack `definition` is in: the Stunt Plane's is its Add-On's, the
    /// bundled original a checkout's content holds once installed
    /// (`python tools/addon_bundle.py install`).
    fn pack_for(definition: &str) -> Option<bri_vehicles::Pack> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = if definition.starts_with("vehicle_stunt_plane:") {
            root.join("content/addons/vehicle_stunt_plane/assets/vehicles.json")
        } else {
            root.join("content/vehicles-pack-012/vehicles.json")
        };
        bri_vehicles::Pack::load(path).ok()
    }
    fn drive_run(definition: &str, seed: u64) -> Result<DriveRun> {
        use bri_vehicles::{
            Occupant, OccupantId, OwnerId as VehicleOwner, Spawn, Transform, VehicleId,
            VehiclesWorld,
        };
        use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder, Vector};
        let pack = pack_for(definition).ok_or_else(|| anyhow::anyhow!("vehicle pack missing"))?;
        // The shipped steering prefs: the mouse steers, nothing returns.
        let steering = bri_sim::session::DEFAULT_STEERING;
        let ground =
            || ColliderBuilder::cuboid(2000., 0.5, 2000.).translation(Vector::new(0., -0.5, 0.));
        let occupant = Occupant {
            id: OccupantId(2),
            owner: VehicleOwner(2),
            body: [1.25, 2.65],
        };
        let spawn = Spawn {
            scale: 1.,
            id: VehicleId(7),
            owner: VehicleOwner(2),
            definition: definition.into(),
            transform: Transform {
                position: [0., 3., 0.],
                ..Default::default()
            },
            spawn_id: None,
            respawn_ticks: None,
        };
        let actor = pack
            .definitions
            .iter()
            .find(|d| d.id == definition)
            .map(|d| d.is_actor().then_some(d.family == bri_vehicles::Family::Horse))
            .ok_or_else(|| anyhow::anyhow!("unknown {definition}"))?;
        // The host: the vehicle settled on the ground with its driver.
        let mut host = VehiclesWorld::new(pack.clone())?;
        let mut world = bri_physics::new_world();
        world.insert(RigidBodyBuilder::fixed(), ground());
        host.spawn(&mut world, spawn.clone())?;
        bri_physics::detect_collisions(&mut world);
        let seat = host.seat_position(&world, VehicleId(7), 0).unwrap();
        host.mount(&world, VehicleId(7), 0, occupant, seat)?;
        let host_step = |host: &mut VehiclesWorld,
                         world: &mut rapier3d::prelude::PhysicsWorld,
                         input: &MoveInput,
                         last: &MoveInput|
         -> Result<()> {
            let controls = match actor {
                Some(horse) => bri_sim::session::actor_controls(input, false, horse),
                None => bri_sim::session::driver_controls(
                    input,
                    (last.yaw, last.pitch),
                    false,
                    (!steering.0, !steering.1),
                ),
            };
            host.set_controls(VehicleOwner(2), OccupantId(2), controls)?;
            host.pre_step(world, &[])?;
            world.step();
            host.post_step(world)?;
            host.drain_intents();
            Ok(())
        };
        for _ in 0..60 {
            host_step(&mut host, &mut world, &MoveInput::default(), &MoveInput::default())?;
        }
        let pose = |host: &VehiclesWorld,
                    world: &rapier3d::prelude::PhysicsWorld,
                    tick: u64,
                    driver_input: u64| {
            let s = host.vehicle_snapshot(world, VehicleId(7)).unwrap();
            bri_sim::session::VehiclePose {
                id: 7,
                tick,
                position: s.shown_transform().position,
                rotation: s.transform.rotation,
                velocity: s.velocity,
                steering: s.steering,
                wheel_suspension: s.wheel_suspension,
                wheel_rotation: s.wheel_rotation,
                wheel_contact: s.wheel_contact,
                wheel_tire: s.wheel_tire,
                turret_aim: s.turret_aim,
                jetting: s.jetting,
                angular_velocity: s.angular_velocity,
                mouse_steering: s.mouse_steering,
                driver_input,
                driver_steering: steering,
                steering_quiet: s.steering_quiet,
                actor: s.actor,
            }
        };
        // The client, seated and predicting from the host's first pose.
        let start = pose(&host, &world, 0, 0);
        let mirror = CollisionMirror::new(Default::default(), vec![ground()], vec![]);
        let rider = PlayerState {
            feet: start.position,
            ..state(0.0, 0.0)
        };
        let mut motion = Motion {
            predictor: Some(Predictor::new(mirror, rider, Default::default())?),
            mounted: true,
            ..Default::default()
        };
        motion.drive(Some((
            7,
            pack,
            bri_sim::prediction::DriveSpawn {
                spawn,
                seat: 0,
                occupant,
                prefs: (!steering.0, !steering.1),
            },
            start.motion(),
        )))?;
        let mut rng = seed;
        let mut random = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng >> 11) as f64 / (1u64 << 53) as f64
        };
        // Quick, sudden mouse moves on a slow weave, full throttle.
        let input_at = |t: f64| {
            let flick = if (t * 1.3).fract() < 0.12 { 0.35 } else { 0.0 };
            MoveInput {
                forward: 1.0,
                yaw: (((t * 2.0).sin() * 1.2 + (t * 1.3).floor() * 0.35 + flick + std::f64::consts::PI)
                    .rem_euclid(std::f64::consts::TAU)
                    - std::f64::consts::PI) as f32,
                pitch: ((t * 3.0).sin() * 0.35) as f32,
                // A mount jumps about as it turns (on a vehicle jump brakes).
                jump: actor.is_some() && (t * 0.9).fract() < 0.1,
                ..Default::default()
            }
        };
        let (mut time, mut host_tick, mut consumed) = (0.0f64, 0u64, 0u64);
        let mut host_last = MoveInput::default();
        let mut pace = bri_sim::session::SeatedPace::default();
        let mut received: BTreeMap<u64, MoveInput> = BTreeMap::new();
        let mut to_host: Vec<(f64, Vec<(u64, MoveInput)>)> = Vec::new();
        let mut to_client: Vec<(f64, bri_sim::session::VehiclePose)> = Vec::new();
        let mut newest: Option<bri_sim::session::VehiclePose> = None;
        let mut run = DriveRun {
            pop: (0.0, 0.0),
            whip: (0.0, 0.0),
            corrections: 0,
            worst: (0.0, 0.0),
            jerk: (0.0, 0.0),
            rough: 0,
        };
        let mut drawn: Option<(Vec3, glam::Quat, Vec3, Vec3)> = None;
        let mut faded: Option<(Vec3, glam::Quat)> = None;
        while time < 8.0 {
            let frame = 0.006 + random() * 0.019;
            time += frame;
            // The host's ticks until now, as `Session::step` runs a seated
            // player: its queued moves at the host's pace, the last again
            // when none has arrived.
            while (host_tick + 1) as f64 / TICK_RATE <= time {
                host_tick += 1;
                let due: Vec<_> = to_host.iter().filter(|(at, _)| *at <= time).cloned().collect();
                to_host.retain(|(at, _)| *at > time);
                for (_, inputs) in due {
                    received.extend(inputs.into_iter().filter(|(s, _)| *s > consumed));
                }
                let runs = pace.runs(received.len());
                let mut input = host_last;
                for _ in 0..runs {
                    if let Some(next) = received.remove(&(consumed + 1)) {
                        consumed += 1;
                        input = next;
                    }
                }
                host_step(&mut host, &mut world, &input, &host_last)?;
                host_last = input;
                if host_tick % POSE_INTERVAL == 0 {
                    let latency = 0.03 + random() * 0.04;
                    to_client.push((time + latency, pose(&host, &world, host_tick, consumed)));
                }
            }
            // The client's frame, as `App::tick` runs it.
            if let Some((sequence, inputs)) = motion.advance(frame as f32, input_at(time), 6)? {
                let first = sequence + 1 - inputs.len() as u64;
                let numbered = (first..).zip(inputs).collect();
                to_host.push((time + 0.03 + random() * 0.04, numbered));
            }
            for (_, pose) in to_client.iter().filter(|(at, _)| *at <= time) {
                if newest.as_ref().is_none_or(|n| n.tick < pose.tick) {
                    newest = Some(pose.clone());
                }
            }
            to_client.retain(|(at, _)| *at > time);
            let before = motion.driven_frame().unwrap();
            if let Some((last_offset, last_turn)) = faded
                && time > 1.0
            {
                let seconds = frame as f32;
                run.whip.0 = run.whip.0.max(motion.drive_offset.distance(last_offset) / seconds);
                run.whip.1 = run.whip.1.max(motion.drive_turn.angle_between(last_turn) / seconds);
            }
            if let Some(pose) = &newest {
                let (offset, turn) = (motion.drive_offset, motion.drive_turn);
                motion.observe_vehicle(pose)?;
                let moved = motion.drive_offset.distance(offset);
                let turned = motion.drive_turn.angle_between(turn);
                if time > 2.0 {
                    run.worst = (run.worst.0.max(moved), run.worst.1.max(turned));
                    if moved > 0.01 || turned > 0.01 {
                        run.corrections += 1;
                    }
                }
            }
            let (_, position, rotation) = motion.driven_frame().unwrap();
            if time > 1.0 {
                run.pop.0 = run.pop.0.max(position.distance(before.1));
                run.pop.1 = run.pop.1.max(rotation.angle_between(before.2));
            }
            faded = Some((motion.drive_offset, motion.drive_turn));
            let seconds = frame as f32;
            let (velocity, spin) = match drawn {
                Some((p, r, ..)) => {
                    let turn = rotation * r.inverse();
                    // The short way round (q and -q are the same turn).
                    let turn = if turn.w < 0.0 { -turn } else { turn };
                    ((position - p) / seconds, turn.to_scaled_axis() / seconds)
                }
                None => (Vec3::ZERO, Vec3::ZERO),
            };
            if let Some((_, _, v, w)) = drawn
                && time > 2.0
            {
                run.jerk.0 = run.jerk.0.max(velocity.distance(v));
                if velocity.distance(v) > 8.0 {
                    run.rough += 1;
                }
                run.jerk.1 = run.jerk.1.max(spin.distance(w));
            }
            drawn = Some((position, rotation, velocity, spin));
        }
        Ok(run)
    }
    /// Max, v0.1.4: the camera and the vehicle it follows jerked apart on
    /// quick moves (horse turns, the stunt plane's pitch, the Magic Carpet).
    /// The chase camera rides the drawn vehicle, so the drawn vehicle must
    /// move smoothly: a host correction never pops it, and the correction
    /// fades gently, over a real connection's timing.
    #[test]
    fn a_driven_vehicle_is_drawn_smoothly_through_corrections() -> Result<()> {
        if vehicle_pack().is_none() {
            eprintln!("vehicle pack missing; skipped");
            return Ok(());
        }
        for definition in [
            "v20.vehicle.magiccarpetvehicle",
            "v20.vehicle.flyingwheeledjeepvehicle",
            "vehicle_stunt_plane:vehicle/stuntplanevehicle",
            "v20.vehicle.horsearmor",
            "v20.vehicle.jeepvehicle",
            "v20.vehicle.tankvehicle",
        ] {
            if pack_for(definition).is_none() {
                eprintln!("{definition}: its pack is not in content/; skipped");
                continue;
            }
            for seed in [0x9e37_79b9_7f4a_7c15, 0x2545_f491_4f6c_dd1d] {
                let run = drive_run(definition, seed)?;
                println!(
                    "{definition}: pop {:.4} units {:.4} rad, whip {:.3} u/s {:.3} rad/s, {} corrections, worst {:.4} units {:.4} rad, jerk {:.2} u/s {:.2} rad/s, {} rough frames",
                    run.pop.0, run.pop.1, run.whip.0, run.whip.1, run.corrections, run.worst.0, run.worst.1, run.jerk.0, run.jerk.1, run.rough
                );
                // Applying a pose never moves the drawn vehicle (f32 noise:
                // `angle_between` reads about 1e-3 for equal rotations).
                assert!(
                    run.pop.0 < 0.005 && run.pop.1 < 0.003,
                    "{definition}: a correction popped the drawn vehicle"
                );
                // The host runs the moves as the client predicted them: once
                // its queue has settled, a correction is rare.
                assert!(
                    run.corrections <= 2,
                    "{definition}: {} visible corrections",
                    run.corrections
                );
                // Max, v0.1.6: the drawn vehicle moves smoothly frame to
                // frame, a horse jumping about too (it stair-stepped on the
                // motor's 32 ms ticks: 45 units/s from one frame to the next).
                assert!(run.rough <= 12, "{definition}: the drawn vehicle jerks on {} frames", run.rough);
                // What remains eases out no faster than the camera can
                // follow, on top of the vehicle's own motion.
                assert!(
                    run.whip.0 <= DRIVE_EASE_SPEED + 1e-3 && run.whip.1 <= DRIVE_EASE_TURN + 1e-3,
                    "{definition}: the correction whips the view"
                );
            }
        }
        Ok(())
    }
}
