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
use anyhow::{Result, ensure};
use bri_net::protocol::{POSE_INTERVAL, PublicWorld};
use bri_sim::{
    player::{MoveInput, PlayerState},
    prediction::{CollisionMirror, Predictor},
};
use bri_world::OwnerId;
use glam::Vec3;
use std::{collections::BTreeMap, f32::consts::PI, sync::Arc};

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

/// The vehicle a client predicts: which one, from which definition, at
/// which scale. Any change starts its prediction again.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DriveTarget {
    pub id: u64,
    pub definition: String,
    pub scale_bits: u32,
}
/// Which vehicle prediction follows. It lives with the prediction it
/// chooses, so whatever resets one (a map change, leaving the game) resets
/// the other.
#[derive(Default)]
pub(crate) struct DriveState {
    pub target: Option<DriveTarget>,
    /// A target whose prediction failed: the host's poses are shown until
    /// the player leaves it.
    pub refused: Option<DriveTarget>,
}
#[derive(Default)]
pub struct Motion {
    pub(crate) drive_state: DriveState,
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
    /// Each remote's recent poses, held poses included (the replica's
    /// history, shared, not copied).
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
    shown_frame: Option<bri_content::passage::PassageFrame>,
    observed_spawn: Option<u64>,
    authoritative_vehicle: Option<(u64, bri_content::passage::PassageFrame)>,
    authoritative_vehicle_tick: u64,
    authoritative_body_frame: Option<bri_content::passage::PassageFrame>,
    frame_transition: Option<u64>,
}

/// The prediction moves the body through an opening on the tick its middle
/// crosses, but draws it between its last two ticks, a little behind. Until
/// the drawn middle crosses too, the body, the look and the camera stay on
/// the near side, so the picture never changes at the crossing: what the
/// near side showed through the opening is what the far side shows.
struct Unshown {
    /// Canonical predicted frame in which this presentation delay ends.
    frame: bri_content::passage::PassageFrame,
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
        let sent = self
            .predictor
            .as_ref()
            .map_or(self.sent_sequence, |p| p.sequence());
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
        self.mounted = mounted || self.authoritative_vehicle.is_some();
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
        let next_id = vehicle.as_ref().map(|(id, ..)| *id);
        if self.driving.is_some()
            && next_id != self.driving
            && self.authoritative_vehicle.map(|(id, _)| id) == self.driving
        {
            // Reliable seat changes may arrive before the body pose. Do not
            // promote an unresolved prediction when its target disappears.
            self.frame_transition = self.driving;
        }
        self.drive_offset = Vec3::ZERO;
        self.drive_turn = glam::Quat::IDENTITY;
        self.driving = None;
        match vehicle {
            Some((id, pack, spawn, motion)) => {
                let current = motion.passage_frame;
                predictor.drive(Some((pack, spawn, motion)))?;
                if let Some((anchor_id, anchor)) = self.authoritative_vehicle {
                    ensure!(
                        anchor_id == id && self.frame_transition.is_none(),
                        "Vehicle frame anchor is pending"
                    );
                    predictor.anchor_drive_frame(
                        anchor,
                        predictor.passage_frame(false),
                        current,
                    )?;
                }
                self.driving = Some(id);
                self.shown_frame
                    .get_or_insert(predictor.passage_frame(true));
            }
            None => {
                predictor.drive(None)?;
                self.shown_frame
                    .get_or_insert(predictor.passage_frame(false));
            }
        }
        Ok(())
    }
    pub fn drive_anchor_ready(
        &self,
        id: u64,
        tick: u64,
        driver_input: u64,
        frame: bri_content::passage::PassageFrame,
    ) -> bool {
        if self.frame_transition.is_none() && self.driving == Some(id) {
            return true;
        }
        self.frame_transition.is_none()
            && self
                .predictor
                .as_ref()
                .is_some_and(|predictor| driver_input <= predictor.sequence())
            && tick >= self.authoritative_vehicle_tick
            && self
                .authoritative_vehicle
                .is_some_and(|(anchor_id, anchor)| {
                    anchor_id == id && frame.revision >= anchor.revision
                })
    }
    /// A target departure is resolved by the body pose, not an obsolete
    /// vehicle's acknowledgement (which is zero after its driver ejects).
    fn finish_target_frame(&mut self, frame: bri_content::passage::PassageFrame) {
        self.unshown = None;
        if let Some(shown) = self.shown_frame
            && let Some(carry) = frame.difference(&shown)
        {
            self.passed = Some(carry * self.passed.unwrap_or(glam::Affine3A::IDENTITY));
        }
        self.shown_frame = Some(frame);
        self.frame_transition = None;
        self.drive_offset = Vec3::ZERO;
        self.drive_turn = glam::Quat::IDENTITY;
        self.correction = Vec3::ZERO;
    }
    pub fn set_drive_prefs(&mut self, prefs: (bool, bool)) {
        if let Some(predictor) = &mut self.predictor {
            predictor.set_drive_prefs(prefs);
        }
    }
    /// Correct the driven vehicle from a newer host pose, carrying the jump
    /// in its drawn place as a correction that fades.
    pub fn observe_vehicle(&mut self, pose: &bri_sim::session::VehiclePose) -> Result<()> {
        if self.driving != Some(pose.id) || pose.tick < self.authoritative_vehicle_tick {
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
        let frame = predictor.passage_frame(true);
        if self.reconcile_presentation_frame(frame) {
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
    /// Replace speculative topology with the frame actually restored/replayed.
    /// Equal frames retain the presentation delay even if its link disappeared.
    fn reconcile_presentation_frame(&mut self, frame: bri_content::passage::PassageFrame) -> bool {
        let Some(shown) = self.shown_frame else {
            self.shown_frame = Some(frame);
            return false;
        };
        let expected = self.unshown.as_ref().map_or(shown, |pending| pending.frame);
        if frame == expected {
            return false;
        }
        self.unshown = None;
        if let Some(carry) = frame.difference(&shown) {
            self.passed = Some(carry * self.passed.unwrap_or(glam::Affine3A::IDENTITY));
        }
        self.shown_frame = Some(frame);
        self.drive_offset = Vec3::ZERO;
        self.drive_turn = glam::Quat::IDENTITY;
        self.correction = Vec3::ZERO;
        true
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
        let (position, rotation) = match &self.unshown {
            Some(unshown) => {
                let back = unshown.carry.inverse();
                match self.predictor.as_ref()?.drive_actor_middle() {
                    Some(middle) => {
                        let forward = rotation * Vec3::NEG_Z;
                        let yaw = forward.x.atan2(-forward.z);
                        (
                            bri_sim::player::carry_feet(&back, position, middle),
                            glam::Quat::from_rotation_y(-bri_content::passage::carried_yaw(
                                &back, yaw,
                            )),
                        )
                    }
                    None => {
                        let (_, turn, _) = back.to_scale_rotation_translation();
                        (back.transform_point3(position), turn * rotation)
                    }
                }
            }
            None => (position, rotation),
        };
        Some((id, position, rotation.normalize()))
    }
    /// The vehicle's canonical travel middle, drawn on the same side as its pose.
    fn drawn_drive_centre(&self) -> Option<Vec3> {
        let (previous, current) = self.predictor.as_ref()?.driven_centres()?;
        let alpha = (self.accumulator / TICK).clamp(0., 1.);
        let centre = previous.lerp(current, alpha);
        Some(
            match &self.unshown {
                Some(unshown) => unshown.carry.inverse().transform_point3(centre),
                None => centre,
            } + self.drive_offset,
        )
    }
    /// The sequence the next input will carry.
    pub fn next_sequence(&self) -> u64 {
        self.predictor
            .as_ref()
            .map_or(self.sent_sequence, |p| p.sequence())
            + 1
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
            }
        }
        // A far player is sent less often, and a pose it held still before
        // moving again arrives in the same interval as the move; the
        // replica keeps both, where the view's latest pose has only one.
        self.remotes = view
            .pose_history
            .iter()
            .filter(|(owner, history)| {
                **owner != view.owner && !history.is_empty() && view.poses.contains_key(owner)
            })
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
        if let Some(predictor) = &self.predictor
            && !predictor.admits_pose(pose.tick, pose.acknowledged_input, pose.player.owner)?
        {
            return Ok(());
        }
        ensure!(
            pose.passage_frame.valid()
                && pose
                    .passage_vehicle
                    .is_none_or(|(id, frame)| id > 0 && frame.valid()),
            "Invalid passage frame"
        );
        let new_body = self
            .observed_spawn
            .is_some_and(|old| old != pose.spawn_tick);
        let entered = pose.passage_vehicle.is_some()
            && pose.passage_vehicle.map(|(id, _)| id)
                != self.authoritative_vehicle.map(|(id, _)| id);
        let departed =
            self.driving.is_some() && self.driving != pose.passage_vehicle.map(|(id, _)| id);
        if departed || new_body {
            if let Some(predictor) = &mut self.predictor {
                predictor.drive(None)?;
            }
            self.driving = None;
        }
        if new_body
            || self.authoritative_vehicle != pose.passage_vehicle
            || self.authoritative_body_frame != Some(pose.passage_frame)
        {
            self.authoritative_vehicle_tick = pose.tick;
        }
        self.authoritative_vehicle = pose.passage_vehicle;
        self.authoritative_body_frame = Some(pose.passage_frame);
        if departed
            || entered
            || new_body
            || self
                .frame_transition
                .is_some_and(|old| pose.passage_vehicle.map(|(id, _)| id) != Some(old))
        {
            self.finish_target_frame(pose.passage_frame);
        }
        self.observed_spawn = Some(pose.spawn_tick);
        if (self.mounted || pose.passage_vehicle.is_some())
            && let Some(predictor) = &mut self.predictor
        {
            predictor.teleport(pose.tick, pose.acknowledged_input, pose.player.clone())?;
            predictor.set_passage_frame(pose.passage_frame)?;
            self.previous = Some(predictor.state().clone());
            self.correction = Vec3::ZERO;
            return Ok(());
        }
        if new_body {
            self.unshown = None;
            self.shown_frame = Some(pose.passage_frame);
            self.correction = Vec3::ZERO;
            self.previous = Some(pose.player.clone());
        }
        if let Some(predictor) = &mut self.predictor {
            if let Some(offset) = predictor.reconcile(
                pose.tick,
                pose.acknowledged_input,
                pose.player.clone(),
                pose.passage_frame,
            )? {
                let frame = predictor.passage_frame(false);
                let current = predictor.state().clone();
                if self.reconcile_presentation_frame(frame) {
                    self.previous = Some(current);
                } else {
                    self.correction += offset;
                    if self.correction.length() > SNAP_DISTANCE {
                        self.correction = Vec3::ZERO;
                        self.previous = Some(current);
                    }
                }
            }
        } else if let Some(mirror) = self.mirror.take() {
            let mut predictor = Predictor::new(mirror, pose.player.clone(), archetypes.clone())?;
            predictor.continue_after(self.sent_sequence);
            // The first pose is authoritative too, so an earlier datagram
            // cannot become this predictor's first admitted correction.
            predictor.teleport(pose.tick, pose.acknowledged_input, pose.player.clone())?;
            predictor.set_passage_frame(pose.passage_frame)?;
            self.shown_frame = Some(pose.passage_frame);
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
            let step =
                (CLOCK_SLEW * TICK_RATE).max(error.abs() / CLOCK_CATCH_UP) * f64::from(seconds);
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
            self.drive_turn = glam::Quat::IDENTITY
                .slerp(self.drive_turn, keep)
                .normalize();
        }
        let Some(predictor) = &mut self.predictor else {
            return Ok(None);
        };
        self.shown_frame
            .get_or_insert_with(|| predictor.passage_frame(self.driving.is_some()));
        self.accumulator += seconds;
        let mut steps = 0;
        if let Some(unshown) = &mut self.unshown {
            unshown.age += seconds;
        }
        let mut mounted_carry = glam::Affine3A::IDENTITY;
        while self.accumulator >= TICK && steps < MAX_STEPS {
            self.accumulator -= TICK;
            // The look is still the near side's while the view is: the body
            // on the far side moves and looks as it turned.
            let input = if self.mounted {
                carried_input(input, &mounted_carry)
            } else {
                input
            };
            let input = match &self.unshown {
                Some(unshown) if self.frame_transition.is_none() => {
                    carried_input(input, &unshown.carry)
                }
                _ => input,
            };
            if self.mounted || self.frame_transition.is_some() {
                // A frame can run several inputs. Once the driven body
                // crosses, its remaining moves use the far-side look too.
                predictor.record(input)?;
                if let Some(crossing) = predictor.take_drive_passed() {
                    if let Some(unshown) = self.unshown.take() {
                        self.shown_frame = Some(unshown.frame);
                        mounted_carry = unshown.carry * mounted_carry;
                        self.passed =
                            Some(unshown.carry * self.passed.unwrap_or(glam::Affine3A::IDENTITY));
                        let (_, turn, _) = unshown.carry.to_scale_rotation_translation();
                        self.drive_offset = unshown.carry.transform_vector3(self.drive_offset);
                        self.drive_turn = (turn * self.drive_turn * turn.inverse()).normalize();
                    }
                    match crossing.entry {
                        Some(entry) => {
                            self.unshown = Some(Unshown {
                                frame: predictor.passage_frame(true),
                                carry: crossing.carry,
                                entry,
                                age: 0.,
                            })
                        }
                        None => {
                            self.shown_frame = Some(predictor.passage_frame(true));
                            mounted_carry = crossing.carry * mounted_carry;
                            self.passed = Some(
                                crossing.carry * self.passed.unwrap_or(glam::Affine3A::IDENTITY),
                            );
                        }
                    }
                }
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
                        self.shown_frame = Some(unshown.frame);
                        self.passed =
                            Some(unshown.carry * self.passed.unwrap_or(glam::Affine3A::IDENTITY));
                    }
                    let to = carry
                        .inverse()
                        .transform_point3(Vec3::from(predictor.state().feet) + lift);
                    match predictor.world().links().passages().first(from, to) {
                        Some((entry, _)) => {
                            self.unshown = Some(Unshown {
                                frame: predictor.passage_frame(false),
                                carry,
                                entry: *entry,
                                age: 0.0,
                            })
                        }
                        None => {
                            self.shown_frame = Some(predictor.passage_frame(false));
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
        let server_tick = self.server_tick();
        let passages = self
            .collision()
            .map(|c| c.links().passages().clone())
            .unwrap_or_default();
        for (owner, pose) in &view.poses {
            if *owner == view.owner {
                continue;
            }
            let render_tick = server_tick.and_then(|tick| self.remote_render_tick(*owner, tick));
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
            Some(unshown) if self.frame_transition.is_none() => {
                state.carried(&unshown.carry.inverse(), predictor.player_middle())
            }
            _ => state,
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
        if self.frame_transition.is_some() {
            return;
        }
        let (Some(unshown), Some(predictor)) = (&self.unshown, &self.predictor) else {
            return;
        };
        let middle = if self.mounted {
            let Some(middle) = self.drawn_drive_centre() else {
                return;
            };
            middle
        } else {
            Vec3::from(self.drawn(predictor).feet) + Vec3::Y * predictor.player_middle()
        };
        if unshown.entry.side(middle) > 0.0 && unshown.age < UNSHOWN_SECONDS {
            return;
        }
        let carry = unshown.carry;
        self.shown_frame = Some(unshown.frame);
        self.unshown = None;
        if self.mounted {
            let (_, turn, _) = carry.to_scale_rotation_translation();
            self.drive_offset = carry.transform_vector3(self.drive_offset);
            self.drive_turn = (turn * self.drive_turn * turn.inverse()).normalize();
        }
        self.passed = Some(carry * self.passed.unwrap_or(glam::Affine3A::IDENTITY));
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
fn sample(
    history: &imbl::Vector<bri_net::protocol::Pose>,
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
            speed_scale: 1.0,
            tick: Default::default(),
            tether: None,
        }
    }
    fn pose(tick: u64, x: f32, yaw: f32) -> bri_net::protocol::Pose {
        bri_net::protocol::Pose {
            passage_frame: Default::default(),
            passage_vehicle: None,
            tick,
            acknowledged_input: 0,
            spawn_tick: 0,
            player: state(x, yaw),
        }
    }
    #[test]
    fn stale_own_spawn_or_ack_cannot_mutate_presentation_before_pose_admission() -> Result<()> {
        for mounted in [false, true] {
            let mut motion = Motion {
                sent_sequence: 10,
                ..Default::default()
            };
            motion.install(CollisionMirror::new(Default::default(), vec![], vec![]));
            // install preserves the input numbering that precedes a map/body.
            motion.sent_sequence = 10;
            let mut current = pose(20, 0., 0.);
            current.spawn_tick = 20;
            current.acknowledged_input = 10;
            motion.observe_local(&current, &Default::default())?;
            motion.set_mounted(mounted);
            let shown = motion.shown_frame;
            let state = motion.predictor.as_ref().unwrap().state().clone();
            let mut old = pose(10, 30., 1.);
            old.spawn_tick = 1;
            old.acknowledged_input = 9;
            motion.observe_local(&old, &Default::default())?;
            assert_eq!(motion.observed_spawn, Some(20));
            assert_eq!(motion.shown_frame, shown);
            assert_eq!(motion.predictor.as_ref().unwrap().state(), &state);
            // A later tick carrying a superseded input ack also stays out.
            old.tick = 21;
            motion.observe_local(&old, &Default::default())?;
            assert_eq!(motion.observed_spawn, Some(20));
            assert_eq!(motion.shown_frame, shown);
            assert_eq!(motion.predictor.as_ref().unwrap().state(), &state);
        }
        Ok(())
    }
    /// Poses every 3 ticks with 80 ms latency plus up to 60 ms jitter,
    /// frames at 144 Hz: the presented server clock never jumps or stalls.
    #[test]
    fn server_clock_runs_smoothly_under_jitter() {
        let frame = 1.0 / 144.0;
        let mut motion = Motion::default();
        let mut rng = 7u64;
        let mut arrivals = std::collections::VecDeque::new();
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
            let due: Vec<_> = arrivals
                .iter()
                .filter(|(at, _)| *at <= time)
                .copied()
                .collect();
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
            motion
                .advance(1.0 / 144.0, MoveInput::default(), 1)
                .unwrap();
            seconds += 1.0 / 144.0;
            assert!(seconds < 5.0, "still behind after {seconds} s");
        }
    }
    #[test]
    fn remote_samples_interpolate_extrapolate_and_wrap_yaw() {
        let history: imbl::Vector<_> = [pose(3, 0.0, 3.0), pose(6, 3.0, -3.0)]
            .into_iter()
            .collect();
        assert_eq!(sample(&history, 0.0, &Default::default()).feet[0], 0.0);
        assert!((sample(&history, 4.5, &Default::default()).feet[0] - 1.5).abs() < 1e-5);
        // Shortest arc through +/-PI rather than spinning through zero.
        assert!(sample(&history, 4.5, &Default::default()).yaw.abs() > 3.0);
        // Extrapolation is bounded to EXTRAPOLATION_TICKS of velocity.
        let far = sample(&history, 1000.0, &Default::default()).feet[0];
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
            let shown = sample(&history, render, &Default::default()).feet[0];
            if let Some(last) = last_shown {
                assert!(shown >= last - 1e-3, "frame {frame}: {shown} after {last}");
            }
            last_shown = Some(shown);
        }
        assert!((motion.remote_delays[&owner] - 51.0).abs() < 1.0);
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
    /// The pack `definition` is in: a plane's is the stand-in plane's
    /// (crates/vehicles/tests/fixtures), in place of the bundled Stunt Plane
    /// no checkout holds.
    fn pack_for(definition: &str) -> Option<bri_vehicles::Pack> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = if definition.starts_with("test_plane:") {
            root.join("crates/vehicles/tests/fixtures/stand-in-plane/assets/vehicles.json")
        } else {
            if !root.join("content").is_dir() {
                return None;
            }
            bri_package::testing::pack_dir(&root.join("content"), "vehicles").join("vehicles.json")
        };
        Some(
            bri_vehicles::Pack::load(&path).unwrap_or_else(|e| panic!("{}: {e:#}", path.display())),
        )
    }
    #[test]
    fn ordinary_mounted_portal_prediction_keeps_the_drawn_centre_and_look_together() -> Result<()> {
        mounted_portal_run(bri_vehicles::testing::pack())
    }
    #[test]
    #[ignore = "needs converted native vehicle pack"]
    fn native_mounted_portal_prediction_keeps_the_drawn_centre_and_look_together() -> Result<()> {
        mounted_portal_run(vehicle_pack().expect("Run the documented importer first"))
    }
    /// Real Session admission, jump-to-board, ordinary driving datagrams,
    /// prediction and delayed pose correction; alpha sampling is rendering only.
    #[test]
    fn a_rejected_mounted_prediction_discards_only_its_obsolete_carry() -> Result<()> {
        mounted_portal_rollback_run(bri_vehicles::testing::pack(), false, false)?;
        mounted_portal_rollback_run(bri_vehicles::testing::pack(), true, false)
    }
    #[test]
    #[ignore = "needs converted native vehicle pack"]
    fn native_rejected_mounted_prediction_discards_only_its_obsolete_carry() -> Result<()> {
        let pack = vehicle_pack().expect("Run the documented importer first");
        mounted_portal_rollback_run(pack.clone(), false, false)?;
        mounted_portal_rollback_run(pack, true, false)
    }
    // Normal Session admission/boarding/driving and a second human's actual
    // Wrench Send. The client sees that reliable world edit before the newer
    // authoritative driven pose, as a real network view does.
    #[test]
    fn overlapping_mounted_frames_use_authoritative_trip_fate_not_pose_distance() -> Result<()> {
        mounted_portal_rollback_run(bri_vehicles::testing::pack(), false, true)?;
        mounted_portal_rollback_run(bri_vehicles::testing::pack(), true, true)
    }
    #[test]
    #[ignore = "needs converted native vehicle pack"]
    fn native_overlapping_mounted_frames_use_authoritative_trip_fate() -> Result<()> {
        let pack = vehicle_pack().expect("Run the documented importer first");
        mounted_portal_rollback_run(pack.clone(), false, true)?;
        mounted_portal_rollback_run(pack, true, true)
    }
    fn mounted_portal_rollback_run(
        pack: bri_vehicles::Pack,
        accepted: bool,
        overlap: bool,
    ) -> Result<()> {
        mounted_portal_departure_run(pack, accepted, overlap, None)
    }
    #[test]
    fn admitted_boarding_settles_walking_fate_before_a_targets_historical_frame() -> Result<()> {
        for accepted in [false, true] {
            let mut motion = Motion::default();
            motion.install(CollisionMirror::new(Default::default(), vec![], vec![]));
            let initial = pose(1, 0., 0.);
            motion.observe_local(&initial, &Default::default())?;
            let carry = glam::Affine3A::from_rotation_translation(
                glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                Vec3::new(0.5, 0., 0.),
            );
            let mut predicted = initial.passage_frame;
            predicted.advance(&carry);
            let predictor = motion.predictor.as_mut().unwrap();
            predictor.record(MoveInput::default())?;
            predictor.set_passage_frame(predicted)?;
            motion.unshown = Some(Unshown {
                frame: predicted,
                carry,
                age: 0.,
                entry: bri_content::passage::Passage {
                    brick: 1,
                    centre: Vec3::ZERO,
                    normal: Vec3::Z,
                    u: Vec3::X,
                    v: Vec3::Y,
                    half: glam::Vec2::splat(4.),
                    carry,
                },
            });
            let mut vehicle = bri_content::passage::PassageFrame::default();
            vehicle.advance(&glam::Affine3A::from_translation(Vec3::new(200., 0., 50.)));
            let mut boarding = initial.clone();
            boarding.tick = 2;
            boarding.acknowledged_input = 1;
            boarding.passage_vehicle = Some((7, vehicle));
            boarding.passage_frame = if accepted {
                predicted
            } else {
                initial.passage_frame
            };
            motion.observe_local(&boarding, &Default::default())?;
            assert!(
                motion.unshown.is_none(),
                "boarding resolves the old walking presentation"
            );
            assert_eq!(motion.shown_frame, Some(boarding.passage_frame));
            assert_eq!(
                motion.predictor.as_ref().unwrap().passage_frame(false),
                boarding.passage_frame,
                "old walking inputs cannot replay in the new control target"
            );
            assert_eq!(motion.take_passed().is_some(), accepted);
            motion.set_mounted(false); // reliable vitals has not caught up yet
            assert!(
                motion.mounted,
                "own pose parks the walking body before late vitals"
            );
            motion.advance(TICK, MoveInput::default(), 1)?;
            assert!(
                motion.take_passed().is_none(),
                "old carry cannot flush a second time"
            );
            assert_eq!(motion.authoritative_vehicle, Some((7, vehicle)));
        }
        Ok(())
    }
    #[test]
    fn mounted_basis_survives_idle_heartbeats_and_rejects_a_previous_driver_pose() -> Result<()> {
        let mut motion = Motion {
            predictor: Some(Predictor::new(
                CollisionMirror::new(bri_sim::testing::definitions(), vec![], vec![]),
                state(0., 0.),
                Default::default(),
            )?),
            ..Default::default()
        };
        let frame = bri_content::passage::PassageFrame::default();
        let own = bri_net::protocol::Pose {
            passage_frame: frame,
            passage_vehicle: Some((7, frame)),
            tick: 10,
            acknowledged_input: 0,
            spawn_tick: 0,
            player: motion.predictor.as_ref().unwrap().state().clone(),
        };
        motion.observe_local(&own, &Default::default())?;
        assert!(
            !motion.drive_anchor_ready(7, 9, 0, frame),
            "previous driver's pose predates boarding"
        );
        assert!(motion.drive_anchor_ready(7, 10, 0, frame));
        let mut heartbeat = own.clone();
        for tick in [22, 34, 46, 120] {
            heartbeat.tick = tick;
            motion.observe_local(&heartbeat, &Default::default())?;
            assert_eq!(
                motion.authoritative_vehicle_tick, 10,
                "unchanged own heartbeats preserve the original same-tick basis"
            );
            assert!(
                motion.drive_anchor_ready(7, 10, 0, frame),
                "cached stationary vehicle remains eligible between sparse packets"
            );
        }
        let carry = glam::Affine3A::from_translation(Vec3::new(0.5, 0., 0.));
        heartbeat.passage_frame.advance(&carry);
        heartbeat
            .passage_vehicle
            .as_mut()
            .unwrap()
            .1
            .advance(&carry);
        heartbeat.tick = 121;
        motion.observe_local(&heartbeat, &Default::default())?;
        assert_eq!(motion.authoritative_vehicle_tick, 121);
        assert!(
            !motion.drive_anchor_ready(7, 120, 0, frame),
            "new route requires its new paired basis"
        );
        let current = heartbeat.passage_vehicle.unwrap().1;
        assert!(motion.drive_anchor_ready(7, 121, 0, current));
        Ok(())
    }
    #[test]
    fn ordinary_same_vehicle_passenger_to_driver_waits_for_the_current_drivers_ack() -> Result<()> {
        passenger_to_driver_run(bri_vehicles::testing::pack())
    }
    #[test]
    #[ignore = "needs converted native vehicle pack"]
    fn native_same_vehicle_passenger_to_driver_waits_for_the_current_drivers_ack() -> Result<()> {
        passenger_to_driver_run(vehicle_pack().expect("Run importer"))
    }
    fn passenger_to_driver_run(pack: bri_vehicles::Pack) -> Result<()> {
        use bri_sim::{
            session::{Command, Session},
            simulation::Simulation,
        };
        use bri_world::{Brick, ContentRef, VehicleSpawn, World};
        use rapier3d::prelude::{ColliderBuilder, Vector};
        let ground =
            || ColliderBuilder::cuboid(200., 0.5, 200.).translation(Vector::new(0., -0.5, 0.));
        let definitions = bri_sim::testing::definitions();
        let mut world = World::new("Seat promotion".into(), "test".into(), vec![[1.; 4]]);
        let mut brick = Brick::new(
            ContentRef::Resolved(bri_sim::testing::VEHICLE_SPAWN.into()),
            [0., 0.1, -12.],
            0,
        );
        brick.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(bri_vehicles::testing::CAR.into()),
            recolor: true,
        }));
        world.bricks.insert(1, brick);
        world.next_brick_id = 2;
        let mut host = Session::new(Simulation::new(world, definitions.clone(), vec![ground()])?);
        host.set_weapon_pack(bri_weapons::testing::pack())?;
        host.set_vehicle_pack(pack.clone(), Vec::new())?;
        let former = host.join("Former driver".into(), Vec3::new(0., 0.05, 0.), true)?;
        let mut former_sequence = 10_000;
        for _ in 0..120 {
            former_sequence += 1;
            host.movement(former, former_sequence, MoveInput::default())?;
            host.step()?;
        }
        for i in 0..600 {
            former_sequence += 1;
            host.movement(
                former,
                former_sequence,
                MoveInput {
                    forward: 1.,
                    jump: (i / 10) % 3 == 0,
                    ..Default::default()
                },
            )?;
            host.step()?;
            if host.mounted(former).is_some() {
                break;
            }
        }
        let mounted = host
            .mounted(former)
            .expect("ordinary first player boards driver seat");
        assert_eq!(mounted.1, 0);
        let owner = host.join("Next driver".into(), Vec3::new(0., 0.05, 0.), true)?;
        let mut sequence = 0;
        for _ in 0..120 {
            former_sequence += 1;
            sequence += 1;
            host.movement(former, former_sequence, MoveInput::default())?;
            host.movement(owner, sequence, MoveInput::default())?;
            host.step()?;
        }
        for i in 0..600 {
            former_sequence += 1;
            sequence += 1;
            host.movement(former, former_sequence, MoveInput::default())?;
            host.movement(
                owner,
                sequence,
                MoveInput {
                    forward: 1.,
                    jump: (i / 10) % 3 == 0,
                    ..Default::default()
                },
            )?;
            host.step()?;
            if host.mounted(owner).is_some() {
                break;
            }
        }
        let passenger_seat = host
            .mounted(owner)
            .expect("ordinary second player boards a free passenger seat");
        assert_eq!(passenger_seat.0, mounted.0);
        assert_ne!(passenger_seat.1, 0);
        let passenger = bri_net::protocol::poses(&host)
            .into_iter()
            .find(|p| p.player.owner == owner)
            .unwrap();
        let cached = host
            .vehicle_poses()
            .into_iter()
            .find(|p| p.id == mounted.0)
            .unwrap();
        assert!(
            cached.driver_input > sequence,
            "real former driver's input numbering is independent"
        );
        let mut mirror = CollisionMirror::new(definitions, vec![ground()], vec![]);
        mirror.sync(&host.simulation().state().bricks)?;
        let mut motion = Motion {
            predictor: Some(Predictor::new(
                mirror,
                passenger.player.clone(),
                Default::default(),
            )?),
            ..Default::default()
        };
        motion.predictor.as_mut().unwrap().continue_after(sequence);
        motion.observe_local(&passenger, &Default::default())?;
        // Real jet control vacates seat zero; the passenger's ordinary NextSeat
        // command then promotes them without changing vehicle or frame identity.
        former_sequence += 1;
        sequence += 1;
        motion
            .predictor
            .as_mut()
            .unwrap()
            .record(MoveInput::default())?;
        host.movement(
            former,
            former_sequence,
            MoveInput {
                jet: true,
                ..Default::default()
            },
        )?;
        host.movement(owner, sequence, MoveInput::default())?;
        host.step()?;
        assert!(host.mounted(former).is_none());
        for command_sequence in 1..=16 {
            host.command(owner, command_sequence, Command::SwitchSeat(-1))?;
            if host.mounted(owner) == Some((mounted.0, 0)) {
                break;
            }
        }
        sequence += 1;
        motion
            .predictor
            .as_mut()
            .unwrap()
            .record(MoveInput::default())?;
        host.movement(owner, sequence, MoveInput::default())?;
        host.step()?;
        assert_eq!(host.mounted(owner), Some((mounted.0, 0)));
        let own = bri_net::protocol::poses(&host)
            .into_iter()
            .find(|p| p.player.owner == owner)
            .unwrap();
        assert_eq!(own.passage_frame, passenger.passage_frame);
        assert_eq!(own.passage_vehicle, passenger.passage_vehicle);
        motion.observe_local(&own, &Default::default())?;
        assert!(
            !motion.drive_anchor_ready(
                cached.id,
                cached.tick,
                cached.driver_input,
                cached.passage_frame
            ),
            "cached former-driver pose waits before bootstrap without poisoning prediction"
        );
        assert!(motion.drive_state.refused.is_none());
        let current = host
            .vehicle_poses()
            .into_iter()
            .find(|p| p.id == mounted.0)
            .unwrap();
        assert_eq!(current.driver_input, sequence);
        assert!(motion.drive_anchor_ready(
            current.id,
            current.tick,
            current.driver_input,
            current.passage_frame
        ));
        let info = host
            .vehicle_infos()
            .into_iter()
            .find(|v| v.id == mounted.0)
            .unwrap();
        motion.drive(Some((
            info.id,
            pack,
            bri_sim::prediction::DriveSpawn {
                spawn: bri_vehicles::Spawn {
                    id: bri_vehicles::VehicleId(info.id),
                    owner: bri_vehicles::OwnerId(owner),
                    definition: info.definition,
                    scale: info.scale,
                    transform: Default::default(),
                    spawn_id: None,
                    respawn_ticks: None,
                },
                seat: 0,
                prefs: (false, false),
            },
            current.motion(),
        )))?;
        motion.observe_vehicle(&current)?;
        assert_eq!(motion.driving(), Some(mounted.0));
        assert!(motion.predictor.as_ref().unwrap().driving());
        assert!(motion.take_passed().is_none());
        Ok(())
    }
    #[test]
    fn ordinary_eject_resolves_rejected_and_accepted_mounted_trip_fate() -> Result<()> {
        for seat_first in [false, true] {
            for accepted in [false, true] {
                mounted_portal_departure_run(
                    bri_vehicles::testing::pack(),
                    accepted,
                    true,
                    Some(seat_first),
                )?;
            }
        }
        Ok(())
    }
    #[test]
    #[ignore = "needs converted native vehicle pack"]
    fn native_eject_resolves_rejected_and_accepted_mounted_trip_fate() -> Result<()> {
        for seat_first in [false, true] {
            for accepted in [false, true] {
                mounted_portal_departure_run(
                    vehicle_pack().expect("Run importer"),
                    accepted,
                    true,
                    Some(seat_first),
                )?;
            }
        }
        Ok(())
    }
    fn mounted_portal_departure_run(
        pack: bri_vehicles::Pack,
        accepted: bool,
        overlap: bool,
        departure: Option<bool>,
    ) -> Result<()> {
        mounted_portal_departure_scenario(pack, accepted, overlap, departure, false)
    }
    #[test]
    fn jet_eject_before_the_same_tick_vehicle_trip_does_not_carry_the_former_rider() -> Result<()> {
        mounted_portal_departure_scenario(
            bri_vehicles::testing::pack(),
            true,
            true,
            Some(true),
            true,
        )
    }
    #[test]
    #[ignore = "needs converted native vehicle pack"]
    fn native_jet_eject_before_the_same_tick_vehicle_trip_does_not_carry_the_former_rider()
    -> Result<()> {
        mounted_portal_departure_scenario(
            vehicle_pack().expect("Run importer"),
            true,
            true,
            Some(true),
            true,
        )
    }
    fn mounted_portal_departure_scenario(
        pack: bri_vehicles::Pack,
        accepted: bool,
        overlap: bool,
        departure: Option<bool>,
        same_tick_eject: bool,
    ) -> Result<()> {
        use bri_sim::{
            session::{
                ActionAim, Command, InspectMode, Notice, Session, ToolAction, WrenchProperties,
            },
            simulation::Simulation,
        };
        use bri_vehicles::{OwnerId as VehicleOwner, Spawn, Transform, VehicleId};
        use bri_world::{Brick, ContentRef, VehicleSpawn, World};
        use rapier3d::prelude::{ColliderBuilder, Vector};
        for definition in [bri_vehicles::testing::CAR, bri_vehicles::testing::HORSE] {
            let ground =
                || ColliderBuilder::cuboid(200., 0.5, 200.).translation(Vector::new(0., -0.5, 0.));
            let mut definitions = bri_sim::testing::definitions();
            const PORTAL: &str = "test/mounted-portal";
            definitions.entries.insert(
                PORTAL.into(),
                bri_sim::testing::portal(PORTAL, Some([14, 1, 30])),
            );
            let mut world = World::new("Mounted portal".into(), "test".into(), vec![[1.; 4]]);
            let mut spawn_brick = Brick::new(
                ContentRef::Resolved(bri_sim::testing::VEHICLE_SPAWN.into()),
                [0., 0.1, -12.],
                0,
            );
            spawn_brick.vehicle = Some(Box::new(VehicleSpawn {
                vehicle: ContentRef::Resolved(definition.into()),
                recolor: true,
            }));
            world.bricks.insert(1, spawn_brick);
            let destination = if overlap {
                (3, [-0.5, 3., -24.25], 0)
            } else {
                (3, [30.25, 3., -24.], 1)
            };
            for (id, position, turns) in [(2, [0., 3., -24.25], 0), destination] {
                let mut portal = Brick::new(ContentRef::Resolved(PORTAL.into()), position, 0);
                portal.quarter_turns = turns;
                portal.name = Some("mounted-door".into());
                world.bricks.insert(id, portal);
            }
            world.next_brick_id = 4;
            let mut host =
                Session::new(Simulation::new(world, definitions.clone(), vec![ground()])?);
            host.set_weapon_pack(bri_weapons::testing::pack())?;
            host.set_vehicle_pack(pack.clone(), Vec::new())?;
            let owner = host.join("Driver".into(), Vec3::new(0., 0.05, 0.), true)?;
            let mut sequence = 0;
            let feed =
                |host: &mut Session, sequence: &mut u64, input: MoveInput, ticks| -> Result<()> {
                    for _ in 0..ticks {
                        *sequence += 1;
                        host.movement(owner, *sequence, input)?;
                        host.step()?;
                    }
                    Ok(())
                };
            feed(&mut host, &mut sequence, MoveInput::default(), 120)?;
            for i in 0..60 {
                feed(
                    &mut host,
                    &mut sequence,
                    MoveInput {
                        forward: 1.,
                        jump: i % 3 == 0,
                        ..Default::default()
                    },
                    10,
                )?;
                if host.mounted(owner).is_some() {
                    break;
                }
            }
            let mounted = host
                .mounted(owner)
                .expect("ordinary jump boards the vehicle");
            feed(&mut host, &mut sequence, MoveInput::default(), 60)?;
            // A second real human opens the source frame with the ordinary
            // wrench trigger before the driving timeline starts. Its Send
            // remains pending until the predicted crossing is observed.
            let editor = host.join("Door editor".into(), Vec3::new(4.5, 0.05, -22.5), true)?;
            host.equip_tool(editor, Some(1))?;
            feed(&mut host, &mut sequence, MoveInput::default(), 60)?;
            let editor_state = host
                .motion_states()
                .into_iter()
                .find(|(p, _)| p.owner == editor)
                .unwrap()
                .0;
            let aim = Vec3::new(3.49, 1.6, -24.25)
                - editor_state.eye(&bri_sim::player::PlayerTuning::default());
            let aim = ActionAim {
                yaw: aim.x.atan2(-aim.z),
                pitch: aim.y.atan2(Vec3::new(aim.x, 0., aim.z).length()),
            };
            // The driver's settling ticks do not renew the editor's input
            // lease. Send a real idle move so step_weapons admits the click.
            host.movement(editor, 1, MoveInput::default())?;
            for (request, down) in [(1, true), (2, false)] {
                host.command_with_aim(editor, request, Command::WeaponTrigger { down }, Some(aim))?;
            }
            feed(&mut host, &mut sequence, MoveInput::default(), 12)?;
            assert!(
                host.take_private_notices()
                    .into_iter()
                    .any(|(who, notice)| {
                        who == editor
                            && matches!(
                                notice,
                                Notice::Inspected {
                                    brick_id: 2,
                                    mode: InspectMode::Wrench,
                                    ..
                                }
                            )
                    }),
                "the ordinary wrench must actually inspect the source portal frame"
            );
            let info = host
                .vehicle_infos()
                .into_iter()
                .find(|v| v.id == mounted.0)
                .unwrap();
            let start = host
                .vehicle_poses()
                .into_iter()
                .find(|v| v.id == mounted.0)
                .unwrap();
            let rider = host
                .motion_states()
                .into_iter()
                .find(|(p, _)| p.owner == owner)
                .unwrap()
                .0;
            let mut mirror = CollisionMirror::new(definitions, vec![ground()], vec![]);
            mirror.sync(&host.simulation().state().bricks)?;
            let mut motion = Motion {
                predictor: Some(Predictor::new(mirror, rider, Default::default())?),
                mounted: true,
                ..Default::default()
            };
            motion.predictor.as_mut().unwrap().continue_after(sequence);
            let initial_pose = bri_net::protocol::poses(&host)
                .into_iter()
                .find(|p| p.player.owner == owner)
                .unwrap();
            motion.observe_local(&initial_pose, &Default::default())?;
            motion.drive(Some((
                info.id,
                pack.clone(),
                bri_sim::prediction::DriveSpawn {
                    spawn: Spawn {
                        id: VehicleId(info.id),
                        owner: VehicleOwner(owner),
                        definition: definition.into(),
                        scale: info.scale,
                        transform: Transform::default(),
                        spawn_id: None,
                        respawn_ticks: None,
                    },
                    seat: usize::from(mounted.1),
                    prefs: (false, false),
                },
                start.motion(),
            )))?;
            let input = MoveInput {
                forward: 1.,
                ..Default::default()
            };
            let mut reached_pending = false;
            for _ in 0..480 {
                let (sent, inputs) = motion
                    .advance(TICK, input, 1)?
                    .expect("one fixed driving tick");
                let pending = motion.unshown.is_some();
                let unlink = |host: &mut Session| -> Result<()> {
                    host.command(
                        editor,
                        3,
                        Command::Tool(ToolAction::SetWrench {
                            brick: 2,
                            properties: WrenchProperties {
                                name: None,
                                raycast: true,
                                colliding: true,
                                visible: true,
                                ..Default::default()
                            },
                        }),
                    )?;
                    assert!(host.simulation().state().bricks[&2].name.is_none());
                    Ok(())
                };
                if pending && !accepted {
                    unlink(&mut host)?;
                }
                if pending && same_tick_eject {
                    // The earlier forward datagram is lost; the newest jet
                    // datagram arrives with redundancy one. The empty vehicle
                    // coasts through after the rider leaves in vehicle_input.
                    let eject = MoveInput {
                        jet: true,
                        ..Default::default()
                    };
                    let eject_sequence = motion.predictor.as_mut().unwrap().record(eject)?;
                    host.movement(owner, eject_sequence, eject)?;
                    host.step()?;
                    assert!(
                        host.mounted(owner).is_none(),
                        "ordinary jet press ejects first"
                    );
                    let vehicle = host
                        .vehicle_poses()
                        .into_iter()
                        .find(|v| v.id == mounted.0)
                        .unwrap();
                    assert!(
                        vehicle.passage_frame.revision > 0,
                        "empty vehicle must actually coast through during this tick: {vehicle:?}"
                    );
                    let own = bri_net::protocol::poses(&host)
                        .into_iter()
                        .find(|p| p.player.owner == owner)
                        .unwrap();
                    assert_eq!(
                        own.passage_frame.revision, 0,
                        "former rider was no longer in the live seat when the vehicle crossed"
                    );
                    motion.observe_local(&own, &Default::default())?;
                    motion.set_mounted(false);
                    motion.drive(None)?;
                    assert!(
                        motion.take_passed().is_none(),
                        "empty vehicle trip cannot turn former rider"
                    );
                    assert!(motion.unshown.is_none());
                    reached_pending = true;
                    break;
                }
                host.movement(owner, sent, *inputs.last().unwrap())?;
                host.step()?;
                let pose = host
                    .vehicle_poses()
                    .into_iter()
                    .find(|v| v.id == mounted.0)
                    .unwrap();
                if !pending {
                    assert!(
                        motion.take_passed().is_none(),
                        "do not announce a trip before the pending split"
                    );
                    continue;
                }
                reached_pending = true;
                assert_eq!(
                    pose.driver_input, sent,
                    "the host actually processed this client's newest ordinary move"
                );
                assert_eq!(host.mounted(owner), Some(mounted));
                if accepted {
                    assert!(
                        pose.passage_frame.revision > 0,
                        "positive control must be an actual host crossing before unlink: {pose:?}"
                    );
                    if !overlap {
                        assert!(
                            pose.position[0] > 20.,
                            "positive control reached destination: {pose:?}"
                        );
                    }
                    unlink(&mut host)?;
                } else {
                    assert_eq!(
                        pose.passage_frame.revision, 0,
                        "host rejected the actual trip"
                    );
                    assert!(
                        pose.position[0].abs() < 5. && pose.position[2] > -24.5,
                        "the authoritative closed source pane rejected the predicted trip: {pose:?}"
                    );
                }
                motion
                    .predictor
                    .as_mut()
                    .unwrap()
                    .sync_world(&host.simulation().state().bricks)?;
                assert!(
                    motion.passages().list.is_empty(),
                    "the replicated wrench edit removed both links"
                );
                if let Some(seat_first) = departure {
                    // The user's next ordinary jet movement ejects. Record the
                    // sent input before its pose arrives, without advancing the
                    // presentation delay while the server processes it.
                    let eject = MoveInput {
                        jet: true,
                        ..Default::default()
                    };
                    let eject_sequence = motion.predictor.as_mut().unwrap().record(eject)?;
                    host.movement(owner, eject_sequence, eject)?;
                    host.step()?;
                    assert!(
                        host.mounted(owner).is_none(),
                        "ordinary jet press must eject"
                    );
                    let own = bri_net::protocol::poses(&host)
                        .into_iter()
                        .find(|p| p.player.owner == owner)
                        .unwrap();
                    assert_eq!(own.passage_vehicle, None);
                    assert_eq!(own.passage_frame.revision, u64::from(accepted));
                    if seat_first {
                        // Reliable vitals/target departure precedes own Pose.
                        motion.set_mounted(false);
                        motion.drive(None)?;
                        motion.advance(TICK, MoveInput::default(), 1)?;
                        assert!(
                            motion.take_passed().is_none(),
                            "unresolved old target cannot publish"
                        );
                    }
                    motion.observe_local(&own, &Default::default())?;
                    motion.set_mounted(false);
                    motion.drive(None)?;
                    let carry = motion.take_passed();
                    assert_eq!(
                        carry.is_some(),
                        accepted,
                        "only the actual old-body trip turns the view across ejection"
                    );
                    if let Some(carry) = carry {
                        let expected = own.passage_frame.transform();
                        assert!(
                            carry.transform_point3(Vec3::new(1., 2., 3.)).abs_diff_eq(
                                expected.transform_point3(Vec3::new(1., 2., 3.)),
                                1e-4
                            )
                        );
                    }
                    assert!(motion.unshown.is_none());
                    assert_eq!(motion.shown_frame, Some(own.passage_frame));
                    assert!(motion.driving().is_none());
                    for _ in 0..16 {
                        motion.advance(TICK, MoveInput::default(), 1)?;
                        assert!(
                            motion.take_passed().is_none(),
                            "old target cannot leak through timeout"
                        );
                    }
                    break;
                }
                motion.observe_vehicle(&pose)?;
                let restored_frame = motion.predictor.as_ref().unwrap().passage_frame(true);
                let drawn_before_stale = motion.driven_frame().unwrap();
                // A real earlier host pose/ack, arriving after this correction,
                // cannot replace canonical travel or resurrect an old carry.
                motion.observe_vehicle(&start)?;
                assert_eq!(
                    motion.predictor.as_ref().unwrap().passage_frame(true),
                    restored_frame
                );
                assert_eq!(motion.driven_frame().unwrap(), drawn_before_stale);
                assert!(
                    motion
                        .predictor
                        .as_mut()
                        .unwrap()
                        .take_drive_passed()
                        .is_none(),
                    "correction/replay does not publish a newly predicted trip"
                );
                let (_, current) = motion.predictor.as_ref().unwrap().driven_centres().unwrap();
                let root = Vec3::from(pose.position);
                assert!(
                    current.distance(root) < 5.,
                    "canonical corrected centre remains close to its actual host body"
                );
                let mut published = usize::from(motion.take_passed().is_some());
                if accepted {
                    // Let the existing bounded presentation delay complete. A
                    // portal disappearing after an accepted crossing must not
                    // return the camera/controls to the source frame.
                    let mut carried = input;
                    for _ in 0..24 {
                        if let Some((sent, inputs)) = motion.advance(TICK, carried, 1)? {
                            host.movement(owner, sent, *inputs.last().unwrap())?;
                            host.step()?;
                        }
                        if let Some(carry) = motion.take_passed() {
                            published += 1;
                            let (yaw, pitch, _) = crate::portal_view::carried_look(
                                (carried.yaw, carried.pitch, 0.),
                                &carry,
                            );
                            carried.yaw = yaw;
                            carried.pitch = pitch;
                        }
                    }
                    assert_eq!(
                        published, 1,
                        "accepted trip survives later link removal and turns input once"
                    );
                    if !overlap {
                        assert!(
                            motion.driven_frame().unwrap().1.x > 20.,
                            "accepted trip remains in canonical destination space"
                        );
                    }
                    assert_eq!(
                        motion.predictor.as_ref().unwrap().passage_frame(true),
                        restored_frame,
                        "removed links and stale datagrams cannot erase an accepted frame"
                    );
                } else {
                    motion.advance(0., input, 1)?;
                    published += usize::from(motion.take_passed().is_some());
                    assert_eq!(
                        published, 0,
                        "a host-rejected predicted trip must never turn the camera/controls"
                    );
                    let drawn = motion.driven_frame().unwrap().1;
                    assert!(
                        drawn.distance(root) < 0.1,
                        "host-rejected correction must draw in canonical source space: drawn={drawn:?}, host={root:?}"
                    );
                    for _ in 0..24 {
                        let (sent, inputs) = motion.advance(TICK, input, 1)?.unwrap();
                        host.movement(owner, sent, *inputs.last().unwrap())?;
                        host.step()?;
                        assert!(
                            motion.take_passed().is_none(),
                            "obsolete carry must not escape through the timeout"
                        );
                        assert!(
                            motion.driven_frame().unwrap().1.x.abs() < 5.,
                            "closed source pane keeps the view in source space"
                        );
                    }
                }
                break;
            }
            assert!(
                reached_pending,
                "ordinary {definition} prediction must actually reach a pending split"
            );
        }
        Ok(())
    }

    fn mounted_portal_run(pack: bri_vehicles::Pack) -> Result<()> {
        use bri_sim::{session::Session, simulation::Simulation};
        use bri_vehicles::{OwnerId as VehicleOwner, Spawn, Transform, VehicleId};
        use bri_world::{Brick, ContentRef, VehicleSpawn, World};
        use rapier3d::prelude::{ColliderBuilder, Vector};
        for definition in [bri_vehicles::testing::CAR, bri_vehicles::testing::HORSE] {
            let ground =
                || ColliderBuilder::cuboid(200., 0.5, 200.).translation(Vector::new(0., -0.5, 0.));
            let mut definitions = bri_sim::testing::definitions();
            const PORTAL: &str = "test/mounted-portal";
            definitions.entries.insert(
                PORTAL.into(),
                bri_sim::testing::portal(PORTAL, Some([14, 1, 30])),
            );
            let mut world = World::new("Mounted portal".into(), "test".into(), vec![[1.; 4]]);
            let mut spawn_brick = Brick::new(
                ContentRef::Resolved(bri_sim::testing::VEHICLE_SPAWN.into()),
                [0., 0.1, -12.],
                0,
            );
            spawn_brick.vehicle = Some(Box::new(VehicleSpawn {
                vehicle: ContentRef::Resolved(definition.into()),
                recolor: true,
            }));
            world.bricks.insert(1, spawn_brick);
            for (id, position, turns) in [(2, [0., 3., -24.25], 0), (3, [30.25, 3., -24.], 1)] {
                let mut portal = Brick::new(ContentRef::Resolved(PORTAL.into()), position, 0);
                portal.quarter_turns = turns;
                portal.name = Some("mounted-door".into());
                world.bricks.insert(id, portal);
            }
            world.next_brick_id = 4;
            let mut host =
                Session::new(Simulation::new(world, definitions.clone(), vec![ground()])?);
            host.set_weapon_pack(bri_weapons::testing::pack())?;
            host.set_vehicle_pack(pack.clone(), Vec::new())?;
            let owner = host.join("Driver".into(), Vec3::new(0., 0.05, 0.), true)?;
            let mut sequence = 0;
            let feed =
                |host: &mut Session, sequence: &mut u64, input: MoveInput, ticks| -> Result<()> {
                    for _ in 0..ticks {
                        *sequence += 1;
                        host.movement(owner, *sequence, input)?;
                        host.step()?;
                    }
                    Ok(())
                };
            feed(&mut host, &mut sequence, MoveInput::default(), 120)?;
            for i in 0..60 {
                feed(
                    &mut host,
                    &mut sequence,
                    MoveInput {
                        forward: 1.,
                        jump: i % 3 == 0,
                        ..Default::default()
                    },
                    10,
                )?;
                if host.mounted(owner).is_some() {
                    break;
                }
            }
            let mounted = host
                .mounted(owner)
                .expect("ordinary jump boards the vehicle");
            feed(&mut host, &mut sequence, MoveInput::default(), 60)?;
            let info = host
                .vehicle_infos()
                .into_iter()
                .find(|v| v.id == mounted.0)
                .unwrap();
            let start = host
                .vehicle_poses()
                .into_iter()
                .find(|v| v.id == mounted.0)
                .unwrap();
            let rider = host
                .motion_states()
                .into_iter()
                .find(|(p, _)| p.owner == owner)
                .unwrap()
                .0;
            let mut mirror = CollisionMirror::new(definitions, vec![ground()], vec![]);
            mirror.sync(&host.simulation().state().bricks)?;
            let mut motion = Motion {
                predictor: Some(Predictor::new(mirror, rider, Default::default())?),
                mounted: true,
                ..Default::default()
            };
            motion.predictor.as_mut().unwrap().continue_after(sequence);
            let initial_pose = bri_net::protocol::poses(&host)
                .into_iter()
                .find(|p| p.player.owner == owner)
                .unwrap();
            motion.observe_local(&initial_pose, &Default::default())?;
            motion.drive(Some((
                info.id,
                pack.clone(),
                bri_sim::prediction::DriveSpawn {
                    spawn: Spawn {
                        id: VehicleId(info.id),
                        owner: VehicleOwner(owner),
                        definition: definition.into(),
                        scale: info.scale,
                        transform: Transform::default(),
                        spawn_id: None,
                        respawn_ticks: None,
                    },
                    seat: usize::from(mounted.1),
                    prefs: (false, false),
                },
                start.motion(),
            )))?;
            let mut input = MoveInput {
                forward: 1.,
                ..Default::default()
            };
            let mut delayed = std::collections::VecDeque::new();
            let mut sampled = false;
            let mut announced = 0;
            let mut saw_near = false;
            let mut saw_far = false;
            for tick in 1..=480u64 {
                let (sent, inputs) = motion
                    .advance(TICK, input, 1)?
                    .expect("one fixed tick input");
                sequence += 1;
                host.movement(owner, sequence, *inputs.last().unwrap())?;
                host.step()?;
                let pose = host
                    .vehicle_poses()
                    .into_iter()
                    .find(|v| v.id == mounted.0)
                    .unwrap();
                let (_, _, predicted) = motion.predictor.as_ref().unwrap().driven().unwrap();
                assert!(
                    Vec3::from(predicted.position).distance(Vec3::from(pose.position)) < 0.08,
                    "ordinary host and prediction agree, {definition} tick{tick}: {predicted:?} vs {pose:?}"
                );
                assert!(
                    glam::Quat::from_array(predicted.rotation)
                        .angle_between(glam::Quat::from_array(pose.rotation))
                        < 0.02
                );
                if tick % 3 == 0 {
                    delayed.push_back((tick + 12, tick, sent, pose.motion()));
                }
                while delayed.front().is_some_and(|(at, ..)| *at <= tick) {
                    let (_, at, ack, pose) = delayed.pop_front().unwrap();
                    motion
                        .predictor
                        .as_mut()
                        .unwrap()
                        .drive_pose(at, ack, &pose)?;
                    assert!(
                        motion
                            .predictor
                            .as_mut()
                            .unwrap()
                            .take_drive_passed()
                            .is_none(),
                        "correction replay cannot announce the trip again"
                    );
                }
                if !sampled && let Some(unshown) = &motion.unshown {
                    let (entry, carry) = (unshown.entry, unshown.carry);
                    let (a, b) = motion.predictor.as_ref().unwrap().driven_centres().unwrap();
                    for alpha in [0., 0.25, 0.5, 0.75] {
                        motion.accumulator = alpha * TICK;
                        let source_middle = carry.inverse().transform_point3(a.lerp(b, alpha));
                        motion.show_crossing();
                        let centre = motion.drawn_drive_centre().unwrap();
                        if entry.side(source_middle) > 0. {
                            assert!(
                                motion.unshown.is_some(),
                                "drawn middle has not crossed at alpha{alpha}"
                            );
                            assert!(centre.distance(source_middle) < 0.001);
                            saw_near = true;
                        } else {
                            assert!(
                                motion.unshown.is_none(),
                                "drawn middle crossed at alpha{alpha}"
                            );
                            assert!(centre.distance(a.lerp(b, alpha)) < 0.001);
                            saw_far = true;
                        }
                        // The Jeep default third-person boom starts from the same
                        // drawn root/heading; its point and look agree with a
                        // continuously unfolded camera across the quarter turn.
                        let (_, root, rotation) = motion.driven_frame().unwrap();
                        let d = pack
                            .definitions
                            .iter()
                            .find(|d| d.id == definition)
                            .unwrap();
                        if !d.is_actor() {
                            let centre =
                                (Vec3::from(d.bounds_min) + Vec3::from(d.bounds_max)) * 0.5;
                            let (eye, yaw, _) = crate::vehicle_camera::driver_view(
                                root,
                                rotation,
                                centre,
                                &d.camera,
                                None,
                                1.,
                                |_, _| Ok(None),
                            )?;
                            let (_, previous, current) =
                                motion.predictor.as_ref().unwrap().driven().unwrap();
                            let raw_root = Vec3::from(previous.position)
                                .lerp(Vec3::from(current.position), alpha);
                            let raw_rotation = glam::Quat::from_array(previous.rotation)
                                .slerp(glam::Quat::from_array(current.rotation), alpha);
                            let (far_eye, far_yaw, _) = crate::vehicle_camera::driver_view(
                                raw_root,
                                raw_rotation,
                                centre,
                                &d.camera,
                                None,
                                1.,
                                |_, _| Ok(None),
                            )?;
                            let unfolded_eye = if motion.unshown.is_some() {
                                carry.transform_point3(eye)
                            } else {
                                eye
                            };
                            assert!(
                                unfolded_eye.distance(far_eye) < 0.002,
                                "camera boom stays in its drawn centre's space at alpha{alpha}"
                            );
                            let heading = Vec3::new(yaw.sin(), 0., -yaw.cos());
                            let unfolded_heading = if motion.unshown.is_some() {
                                carry.transform_vector3(heading)
                            } else {
                                heading
                            };
                            assert!(
                                unfolded_heading.distance(Vec3::new(
                                    far_yaw.sin(),
                                    0.,
                                    -far_yaw.cos()
                                )) < 0.002
                            );
                        }
                    }
                    motion.accumulator = 0.;
                    sampled = true;
                }
                if let Some(carry) = motion.take_passed() {
                    let (yaw, pitch, _) =
                        crate::portal_view::carried_look((input.yaw, input.pitch, 0.), &carry);
                    input.yaw = yaw;
                    input.pitch = pitch;
                    announced += 1;
                }
                if sampled && motion.unshown.is_none() && announced > 0 {
                    saw_far = true;
                    break;
                }
            }
            assert!(
                sampled && saw_near && saw_far,
                "ordinary {definition} crossing samples both sides"
            );
            assert_eq!(announced, 1, "one visible crossing turns the input once");
            assert_eq!(host.mounted(owner), Some(mounted));
        }
        Ok(())
    }

    fn drive_run(pack: bri_vehicles::Pack, definition: &str, seed: u64) -> Result<DriveRun> {
        use bri_vehicles::{
            Occupant, OccupantId, OwnerId as VehicleOwner, Spawn, Transform, VehicleId,
            VehiclesWorld,
        };
        use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder, Vector};
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
            .map(|d| {
                d.is_actor()
                    .then_some(d.family == bri_vehicles::Family::Horse)
            })
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
            host_step(
                &mut host,
                &mut world,
                &MoveInput::default(),
                &MoveInput::default(),
            )?;
        }
        let pose = |host: &VehiclesWorld,
                    world: &rapier3d::prelude::PhysicsWorld,
                    tick: u64,
                    driver_input: u64| {
            let s = host.vehicle_snapshot(world, VehicleId(7)).unwrap();
            bri_sim::session::VehiclePose {
                passage_frame: Default::default(),
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
                yaw: (((t * 2.0).sin() * 1.2
                    + (t * 1.3).floor() * 0.35
                    + flick
                    + std::f64::consts::PI)
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
                let due: Vec<_> = to_host
                    .iter()
                    .filter(|(at, _)| *at <= time)
                    .cloned()
                    .collect();
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
                run.whip.0 = run
                    .whip
                    .0
                    .max(motion.drive_offset.distance(last_offset) / seconds);
                run.whip.1 = run
                    .whip
                    .1
                    .max(motion.drive_turn.angle_between(last_turn) / seconds);
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
        use bri_vehicles::testing;
        let synthetic = testing::pack();
        // Ground stand-ins and the checked-in plane always exercise the
        // acceptance assertions in content-free CI. Authored air vehicles
        // below retain their original tuning and thresholds when installed.
        let mut cases: Vec<_> = [testing::CAR, testing::TANK, testing::HORSE]
            .into_iter()
            .map(|id| (id, synthetic.clone()))
            .collect();
        if let Some(original) = vehicle_pack() {
            for definition in [
                "v20.vehicle.magiccarpetvehicle",
                "v20.vehicle.flyingwheeledjeepvehicle",
                "v20.vehicle.horsearmor",
                "v20.vehicle.jeepvehicle",
                "v20.vehicle.tankvehicle",
            ] {
                cases.push((definition, original.clone()));
            }
        }
        let plane = "test_plane:vehicle/standinplane";
        cases.push((plane, pack_for(plane).expect("checked-in plane fixture")));
        for (definition, pack) in cases {
            for seed in [0x9e37_79b9_7f4a_7c15, 0x2545_f491_4f6c_dd1d] {
                let run = drive_run(pack.clone(), definition, seed)?;
                println!(
                    "{definition}: pop {:.4} units {:.4} rad, whip {:.3} u/s {:.3} rad/s, {} corrections, worst {:.4} units {:.4} rad, jerk {:.2} u/s {:.2} rad/s, {} rough frames",
                    run.pop.0,
                    run.pop.1,
                    run.whip.0,
                    run.whip.1,
                    run.corrections,
                    run.worst.0,
                    run.worst.1,
                    run.jerk.0,
                    run.jerk.1,
                    run.rough
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
                assert!(
                    run.rough <= 12,
                    "{definition}: the drawn vehicle jerks on {} frames",
                    run.rough
                );
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
