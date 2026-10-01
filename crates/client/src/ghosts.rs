//! Smooth presentation of replicated moving bodies between host updates.
//!
//! The host replicates projectiles (thrown and kicked balls included),
//! dropped items and package entities with its 20 Hz state deltas. Drawing
//! those snapshots directly moves them in 50 ms steps. Torque clients never
//! did: a ghosted `Projectile` or `Item` runs its own `processTick` on the
//! client from the newest update, `interpolateTick` blends between ticks at
//! the frame rate, and a disagreeing update warps the rendered object onto
//! the new path instead of popping.
//!
//! [`Tracks`] does the same for any body keyed by a stable id. A
//! [`Mode::Simulated`] body (anything with a velocity) is advanced from its
//! newest update to the current estimated server tick with that update's
//! velocity and acceleration, stopping at the first surface the caller's
//! sweep reports. A [`Mode::Interpolated`] body (no velocity) is drawn a
//! little in the past between buffered updates. Either way a new update
//! keeps the drawn pose continuous and decays the disagreement over a few
//! frames; teleports snap. [`Ghosts`] adapts the replicated weapon view and
//! package entities onto tracks, so any future replicated rigid body only
//! needs to feed `observe` and read `pose`.
use bri_content::passage::{PAST, Passages};
use bri_sim::session::{EntityInfo, WeaponView};
use glam::{Quat, Vec3};
use std::collections::{BTreeMap, VecDeque};

const TICK_RATE: f64 = 120.0;
const TICK: f32 = 1.0 / 120.0;
/// Updates kept per body.
const HISTORY: usize = 8;
/// Nominal ticks between host updates (`server.rs` sends deltas every 6).
const NOMINAL_INTERVAL: f64 = 6.0;
/// Unless its update says otherwise, simulation beyond the newest update
/// stops after this many ticks: a body the host stopped describing holds
/// still rather than flying off.
pub const DEFAULT_HORIZON: f64 = 36.0;
/// Simulated flight checks one chord of its arc per this many ticks.
const CHUNK: f64 = 6.0;
/// Surfaces met per update before a simulated body stops.
const MAX_CONTACTS: u8 = 16;
/// Corrections decay at this rate per second (a half-life near 50 ms).
const CORRECTION_RATE: f32 = 14.0;
/// Disagreements larger than this are teleports (respawn, pickup) and snap.
const SNAP_DISTANCE: f32 = 6.0;
/// The presented clock follows the estimate by running up to this much
/// faster or slower, so a revised estimate never jumps bodies along.
const CLOCK_SLEW: f64 = 0.05;
/// Clock disagreements beyond this many ticks snap (a stall, a map change).
const CLOCK_SNAP: f64 = 60.0;
/// Larger disagreements close faster, over about this many seconds.
const CLOCK_CATCH_UP: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Projectile,
    Drop,
    Entity,
    /// Any other replicated rigid body (for example pushable bricks).
    Body,
}
pub type Key = (Kind, u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Advance from the newest update by its velocity and acceleration.
    Simulated,
    /// Draw between buffered updates, slightly in the past.
    Interpolated,
}

/// One authoritative state of a body at a host tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Update {
    pub position: Vec3,
    pub velocity: Vec3,
    pub acceleration: Vec3,
    pub rotation: Quat,
    /// How it bounces off surfaces; `None` stops at the first one.
    pub bounce: Option<Bounce>,
    /// Ticks past this update the body keeps simulating: a projectile flies
    /// out its lifetime from one update, as Torque's ghosts did.
    pub horizon: f64,
}
impl Update {
    pub fn at(position: Vec3) -> Self {
        Self {
            position,
            velocity: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            bounce: None,
            horizon: DEFAULT_HORIZON,
        }
    }
}
/// Torque's `Projectile::simulate` bounce: reflect off the surface, lose
/// tangential speed to friction, then scale by elasticity; slower than
/// `rest_speed` afterwards, the body stops.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounce {
    pub elasticity: f32,
    pub friction: f32,
    pub rest_speed: f32,
}
/// The first surface on a segment: where, its normal, and the fraction of
/// the segment travelled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub position: Vec3,
    pub normal: Vec3,
    pub fraction: f32,
    /// An opening of a linked brick rather than a surface: the body goes
    /// on out of its partner, carried by this, as the host flies it.
    pub carry: Option<glam::Affine3A>,
}
impl Hit {
    /// What a body moving from `from` to `to` meets first: `solid`, the
    /// surface `sweep` finds, or an opening of `passages` before it.
    pub fn first(
        passages: &Passages,
        from: Vec3,
        to: Vec3,
        sweep: impl FnOnce(Vec3, Vec3) -> Option<Hit>,
    ) -> Option<Hit> {
        let solid = sweep(from, to);
        match passages.first(from, to) {
            Some((opening, t)) if solid.is_none_or(|s| s.fraction > t) => Some(Hit {
                position: from.lerp(to, t),
                normal: opening.normal,
                fraction: t,
                carry: Some(opening.carry),
            }),
            _ => solid,
        }
    }
}

/// A body's pose for this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub position: Vec3,
    pub velocity: Vec3,
    pub rotation: Quat,
    /// Ticks the presented pose lies past its base update (for ages).
    pub ahead: f64,
}

struct Track {
    mode: Mode,
    updates: VecDeque<(u64, Update)>,
    /// Drawn pose minus the authoritative path, decaying to zero.
    offset: Vec3,
    turn: Quat,
    /// Simulated flight from the newest update, at a whole chunk.
    checkpoint: Option<Flying>,
}

/// A simulated body `ticks` after its update.
#[derive(Clone, Copy, Debug)]
struct Flying {
    ticks: f64,
    position: Vec3,
    velocity: Vec3,
    contacts: u8,
    stopped: bool,
}
impl Flying {
    fn start(update: &Update) -> Self {
        Self {
            ticks: 0.0,
            position: update.position,
            velocity: update.velocity,
            contacts: 0,
            stopped: update.velocity == Vec3::ZERO && update.acceleration == Vec3::ZERO,
        }
    }
}

/// Estimated host clock for one update stream, like `Motion`'s pose clock:
/// it tracks the least-delayed arrivals and measures how late others are.
#[derive(Default)]
struct Clock {
    seconds: f64,
    /// Estimated `host_tick - seconds * TICK_RATE`.
    offset: Option<f64>,
    /// The offset presented, slewing toward `offset`.
    shown: Option<f64>,
    /// The interpolation delay presented, slewing toward `wanted_delay`.
    delay: f64,
    newest: Option<u64>,
    /// Smoothed ticks between updates.
    interval: f64,
    /// Recent worst lateness of arrivals behind the least-delayed ones, in
    /// ticks, slowly forgotten.
    jitter: f64,
}
impl Clock {
    fn observe(&mut self, tick: u64) {
        if self.newest.is_some_and(|newest| tick <= newest) {
            return;
        }
        if let Some(newest) = self.newest {
            let spacing = ((tick - newest) as f64).min(4.0 * NOMINAL_INTERVAL);
            self.interval += (spacing - self.interval) * 0.1;
        } else {
            self.interval = NOMINAL_INTERVAL;
        }
        self.newest = Some(tick);
        let sample = tick as f64 - self.seconds * TICK_RATE;
        self.offset = Some(match self.offset {
            Some(offset) if sample < offset => {
                self.jitter = (self.jitter * 0.99).max(offset - sample);
                offset + (sample - offset) * 0.02
            }
            _ => {
                self.jitter *= 0.99;
                sample
            }
        });
    }
    fn advance(&mut self, seconds: f64) {
        self.seconds += seconds;
        let Some(offset) = self.offset else {
            return;
        };
        let slew = |shown: f64, target: f64| {
            let error = target - shown;
            let step = (CLOCK_SLEW * TICK_RATE).max(error.abs() / CLOCK_CATCH_UP) * seconds;
            if error.abs() > CLOCK_SNAP {
                target
            } else {
                shown + error.clamp(-step, step)
            }
        };
        let shown = self.shown.unwrap_or(offset);
        self.shown = Some(slew(shown, offset));
        let wanted = self.wanted_delay();
        self.delay = if self.delay == 0.0 {
            wanted
        } else {
            slew(self.delay, wanted)
        };
    }
    fn now(&self) -> Option<f64> {
        self.shown
            .or(self.offset)
            .map(|o| self.seconds * TICK_RATE + o)
    }
    /// How far interpolated bodies trail the clock: one update interval
    /// plus room for the measured lateness.
    fn delay(&self) -> f64 {
        if self.delay > 0.0 {
            self.delay
        } else {
            self.wanted_delay()
        }
    }
    fn wanted_delay(&self) -> f64 {
        let interval = self.interval.max(1.0);
        (interval + self.jitter + 1.0).clamp(interval, 4.0 * interval)
    }
}

#[derive(Default)]
pub struct Tracks {
    clock: Clock,
    tracks: BTreeMap<Key, Track>,
}
impl Tracks {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    /// Advance local time by one rendered frame.
    pub fn advance(&mut self, seconds: f32) {
        let seconds = if seconds.is_finite() {
            seconds.clamp(0.0, 0.25)
        } else {
            0.0
        };
        self.clock.advance(f64::from(seconds));
        let decay = (-CORRECTION_RATE * seconds).exp();
        for track in self.tracks.values_mut() {
            track.offset *= decay;
            if track.offset.length_squared() < 1e-10 {
                track.offset = Vec3::ZERO;
            }
            track.turn = Quat::IDENTITY.slerp(track.turn, decay).normalize();
        }
    }
    /// Note that the host sent an update batch at `tick`.
    pub fn arrived(&mut self, tick: u64) {
        self.clock.observe(tick);
    }
    /// The estimated current host tick.
    pub fn now(&self) -> Option<f64> {
        self.clock.now()
    }
    pub fn contains(&self, key: Key) -> bool {
        self.tracks.contains_key(&key)
    }
    /// Record a body's authoritative state at `tick`. The drawn pose stays
    /// where it was; the difference decays over the next frames.
    pub fn observe(
        &mut self,
        key: Key,
        tick: u64,
        mode: Mode,
        update: Update,
        sweep: &mut impl FnMut(Vec3, Vec3) -> Option<Hit>,
    ) {
        let now = self.clock.now();
        let delay = self.clock.delay();
        let track = self.tracks.entry(key).or_insert_with(|| Track {
            mode,
            updates: VecDeque::new(),
            offset: Vec3::ZERO,
            turn: Quat::IDENTITY,
            checkpoint: None,
        });
        if track.updates.back().is_some_and(|(t, _)| *t >= tick) {
            return;
        }
        // Where the old path would draw the body this frame.
        let drawn = now.filter(|_| !track.updates.is_empty()).map(|now| {
            let old = track.target(now, delay, sweep);
            (
                old.position + track.offset,
                (track.turn * old.rotation).normalize(),
            )
        });
        if track.mode != mode {
            track.mode = mode;
            track.updates.clear();
        }
        if track.updates.len() == HISTORY {
            track.updates.pop_front();
        }
        track.updates.push_back((tick, update));
        track.checkpoint = None;
        let (Some((position, rotation)), Some(now)) = (drawn, now) else {
            return;
        };
        let target = track.target(now, delay, sweep);
        let offset = position - target.position;
        if offset.is_finite() && offset.length() <= SNAP_DISTANCE {
            track.offset = offset;
            track.turn = (rotation * target.rotation.inverse()).normalize();
        } else {
            track.offset = Vec3::ZERO;
            track.turn = Quat::IDENTITY;
        }
    }
    /// Forget bodies the newest batch no longer contains.
    pub fn retain(&mut self, mut keep: impl FnMut(&Key) -> bool) {
        self.tracks.retain(|key, _| keep(key));
    }
    /// This frame's pose of a body.
    pub fn pose(
        &mut self,
        key: Key,
        sweep: &mut impl FnMut(Vec3, Vec3) -> Option<Hit>,
    ) -> Option<Pose> {
        let now = self.clock.now();
        let delay = self.clock.delay();
        let track = self.tracks.get_mut(&key)?;
        let mut pose = match now {
            Some(now) => track.target(now, delay, sweep),
            None => {
                let (_, update) = track.updates.back()?;
                Pose {
                    position: update.position,
                    velocity: update.velocity,
                    rotation: update.rotation,
                    ahead: 0.0,
                }
            }
        };
        pose.position += track.offset;
        pose.rotation = (track.turn * pose.rotation).normalize();
        Some(pose)
    }
}
impl Track {
    fn target(
        &mut self,
        now: f64,
        delay: f64,
        sweep: &mut impl FnMut(Vec3, Vec3) -> Option<Hit>,
    ) -> Pose {
        let (newest_tick, newest) = *self.updates.back().expect("tracks hold an update");
        match self.mode {
            Mode::Simulated => {
                let ticks = (now - newest_tick as f64).clamp(0.0, newest.horizon.max(0.0));
                let from = self
                    .checkpoint
                    .filter(|c| c.ticks <= ticks)
                    .unwrap_or_else(|| Flying::start(&newest));
                let (checkpoint, end) = fly(&newest, from, ticks, sweep);
                self.checkpoint = Some(checkpoint);
                Pose {
                    position: end.position,
                    velocity: end.velocity,
                    rotation: newest.rotation,
                    ahead: ticks,
                }
            }
            Mode::Interpolated => {
                let at = now - delay;
                let pairs = self.updates.iter().zip(self.updates.iter().skip(1));
                for (&(ta, a), &(tb, b)) in pairs {
                    if at <= tb as f64 && at >= ta as f64 {
                        let t = ((at - ta as f64) / (tb - ta).max(1) as f64) as f32;
                        return Pose {
                            position: a.position.lerp(b.position, t),
                            velocity: (b.position - a.position) / ((tb - ta) as f32 * TICK),
                            rotation: a.rotation.slerp(b.rotation, t).normalize(),
                            ahead: at - ta as f64,
                        };
                    }
                }
                let (first_tick, first) = *self.updates.front().unwrap();
                if at < first_tick as f64 {
                    return Pose {
                        position: first.position,
                        velocity: Vec3::ZERO,
                        rotation: first.rotation,
                        ahead: 0.0,
                    };
                }
                // Past the newest update: continue its last motion briefly.
                let velocity = match self.updates.len() {
                    0 | 1 => Vec3::ZERO,
                    n => {
                        let (ta, a) = self.updates[n - 2];
                        (newest.position - a.position) / ((newest_tick - ta).max(1) as f32 * TICK)
                    }
                };
                let limit = NOMINAL_INTERVAL.max(delay);
                let ahead = (at - newest_tick as f64).clamp(0.0, limit);
                Pose {
                    position: newest.position + velocity * (ahead as f32 * TICK),
                    velocity,
                    rotation: newest.rotation,
                    ahead,
                }
            }
        }
    }
}

/// `update` advanced `ticks` host ticks the way the host integrates it
/// (velocity then position each tick), meeting surfaces through `sweep`.
pub fn simulate(
    update: &Update,
    ticks: f64,
    sweep: &mut impl FnMut(Vec3, Vec3) -> Option<Hit>,
) -> Pose {
    let ticks = ticks.clamp(0.0, update.horizon.max(0.0));
    let (_, end) = fly(update, Flying::start(update), ticks, sweep);
    Pose {
        position: end.position,
        velocity: end.velocity,
        rotation: update.rotation,
        ahead: ticks,
    }
}

/// Fly `from` on to `ticks` after `update`, one chord per whole chunk.
/// Returns the state at the last whole chunk (to resume from) and at `ticks`.
fn fly(
    update: &Update,
    mut from: Flying,
    ticks: f64,
    sweep: &mut impl FnMut(Vec3, Vec3) -> Option<Hit>,
) -> (Flying, Flying) {
    let mut checkpoint = from;
    while from.ticks < ticks && !from.stopped {
        let chunk_end = ((from.ticks / CHUNK).floor() + 1.0) * CHUNK;
        let to = chunk_end.min(ticks);
        step(update, &mut from, to, sweep);
        if from.ticks == chunk_end {
            checkpoint = from;
        }
    }
    from.ticks = ticks;
    if checkpoint.ticks > ticks {
        checkpoint = Flying::start(update);
    }
    (checkpoint, from)
}

/// Advance one chord of flight to `to` ticks, bouncing off what it meets.
fn step(
    update: &Update,
    body: &mut Flying,
    to: f64,
    sweep: &mut impl FnMut(Vec3, Vec3) -> Option<Hit>,
) {
    let a = update.acceleration;
    while body.ticks < to && !body.stopped {
        let (p, v) = (body.position, body.velocity);
        let span = ((to - body.ticks) / TICK_RATE) as f32;
        let at = |t: f32| p + v * t + a * (0.5 * (t * t + t * TICK));
        let end = at(span);
        let Some(hit) = sweep(p, end) else {
            body.position = end;
            body.velocity = v + a * span;
            body.ticks = to;
            return;
        };
        let t = span * hit.fraction.clamp(0.0, 1.0);
        body.position = hit.position;
        body.velocity = v + a * t;
        body.ticks += f64::from(t) * TICK_RATE;
        body.contacts += 1;
        if let Some(carry) = hit.carry {
            // Out of the partner, turned, a hair past its plane so the rest
            // of the chord does not go back in.
            let (_, turn, _) = carry.to_scale_rotation_translation();
            body.velocity = turn * body.velocity;
            body.position =
                carry.transform_point3(hit.position) + body.velocity.normalize_or_zero() * PAST;
            body.stopped = body.contacts >= MAX_CONTACTS;
            continue;
        }
        let normal = hit.normal.normalize_or_zero();
        match update.bounce {
            Some(bounce) if normal != Vec3::ZERO && body.contacts < MAX_CONTACTS => {
                let velocity = body.velocity;
                let reflected = velocity - normal * velocity.dot(normal) * 2.0;
                let tangent = reflected - normal * reflected.dot(normal);
                body.velocity = (reflected - tangent * bounce.friction) * bounce.elasticity;
                body.position += normal * 0.002;
                if body.velocity.length() < bounce.rest_speed {
                    body.velocity = Vec3::ZERO;
                    body.stopped = true;
                }
            }
            _ => {
                body.velocity = Vec3::ZERO;
                body.stopped = true;
            }
        }
    }
}

/// How a projectile definition flies between updates.
#[derive(Clone, Copy, Debug)]
pub struct Flight {
    pub acceleration: Vec3,
    pub lifetime: u32,
    pub bounce: Option<Bounce>,
}

/// Smoothed copies of the replicated weapon view and package entities.
#[derive(Default)]
pub struct Ghosts {
    tracks: Tracks,
    tick: Option<u64>,
    weapons: WeaponView,
    entities: BTreeMap<u64, EntityInfo>,
    /// Each simulated body's replicated state when last observed, and a
    /// projectile's age then. Unchanged state is not a new update: the
    /// client keeps flying the body from where the host last described it,
    /// so the host may send a projectile once and then only corrections.
    sent: BTreeMap<Key, (Vec3, Vec3, u32)>,
}
impl Ghosts {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    /// Advance by one frame and present every body at the frame rate.
    /// `projectile` describes a projectile definition's flight; `sweep`
    /// finds the first surface on a segment.
    pub fn update(
        &mut self,
        seconds: f32,
        tick: u64,
        weapons: &WeaponView,
        entities: &BTreeMap<u64, EntityInfo>,
        projectile: impl Fn(&str) -> Option<Flight>,
        mut sweep: impl FnMut(Vec3, Vec3) -> Option<Hit>,
    ) {
        self.tracks.advance(seconds);
        if self.tick != Some(tick) {
            if self.tick.is_some_and(|t| tick < t) {
                self.tracks.clear();
            }
            self.tick = Some(tick);
            self.tracks.arrived(tick);
            let tracks = &self.tracks;
            let sent = &mut self.sent;
            let mut fresh =
                |key, state| sent.insert(key, state) != Some(state) || !tracks.contains(key);
            let mut observed = Vec::new();
            for p in &weapons.projectiles {
                let key = (Kind::Projectile, p.id);
                let velocity = if p.stuck { Vec3::ZERO } else { p.velocity };
                if !fresh(key, (p.position, velocity, p.age)) {
                    continue;
                }
                let flight = projectile(&p.definition).filter(|_| !p.stuck);
                observed.push((
                    key,
                    Update {
                        position: p.position,
                        velocity,
                        acceleration: flight.map_or(Vec3::ZERO, |f| f.acceleration),
                        rotation: crate::world_items::projectile_rotation(p.velocity),
                        bounce: flight.and_then(|f| f.bounce),
                        // It flies out its lifetime unless the host says otherwise.
                        horizon: flight.map_or(DEFAULT_HORIZON, |f| {
                            f64::from(f.lifetime.saturating_sub(p.age))
                        }),
                    },
                ));
            }
            for d in &weapons.drops {
                let key = (Kind::Drop, d.id);
                // `Item::updatePos`: gravity 20 while the item moves at all.
                let moving = d.velocity.length_squared() >= 0.000001;
                let velocity = if moving { d.velocity } else { Vec3::ZERO };
                if !fresh(key, (d.position, velocity, 0)) {
                    continue;
                }
                observed.push((
                    key,
                    Update {
                        position: d.position,
                        velocity,
                        acceleration: if moving {
                            Vec3::NEG_Y * 20.0
                        } else {
                            Vec3::ZERO
                        },
                        rotation: d.rotation,
                        // Elasticity 0.2 and friction 0.6, as on the host.
                        bounce: Some(Bounce {
                            elasticity: 0.2,
                            friction: 0.6,
                            rest_speed: 0.5,
                        }),
                        horizon: 10.0 * TICK_RATE,
                    },
                ));
            }
            for (key, update) in observed {
                self.tracks
                    .observe(key, tick, Mode::Simulated, update, &mut sweep);
            }
            for e in entities.values() {
                self.tracks.observe(
                    (Kind::Entity, e.id),
                    tick,
                    Mode::Interpolated,
                    Update {
                        rotation: Quat::from_rotation_y(e.yaw),
                        ..Update::at(Vec3::from(e.position))
                    },
                    &mut sweep,
                );
            }
            // Vanished bodies go at once, together with their removal cues.
            let live: std::collections::BTreeSet<Key> = weapons
                .projectiles
                .iter()
                .map(|p| (Kind::Projectile, p.id))
                .chain(weapons.drops.iter().map(|d| (Kind::Drop, d.id)))
                .chain(entities.keys().map(|id| (Kind::Entity, *id)))
                .collect();
            self.tracks
                .retain(|key| key.0 == Kind::Body || live.contains(key));
            self.sent.retain(|key, _| live.contains(key));
            self.weapons = weapons.clone();
            self.entities = entities.clone();
        }
        for p in &mut self.weapons.projectiles {
            if let Some(pose) = self.tracks.pose((Kind::Projectile, p.id), &mut sweep) {
                p.position = pose.position;
                if !p.stuck {
                    p.velocity = pose.velocity;
                }
                let lifetime = projectile(&p.definition).map_or(u32::MAX, |f| f.lifetime);
                let base = self
                    .sent
                    .get(&(Kind::Projectile, p.id))
                    .map_or(p.age, |(_, _, age)| *age);
                p.age = base
                    .saturating_add(pose.ahead as u32)
                    .min(lifetime.saturating_sub(1).max(base));
            }
        }
        for d in &mut self.weapons.drops {
            if let Some(pose) = self.tracks.pose((Kind::Drop, d.id), &mut sweep) {
                d.position = pose.position;
                d.rotation = pose.rotation;
            }
        }
        for e in self.entities.values_mut() {
            if let Some(pose) = self.tracks.pose((Kind::Entity, e.id), &mut sweep) {
                e.position = pose.position.to_array();
                let (y, _, _) = pose.rotation.to_euler(glam::EulerRot::YXZ);
                e.yaw = y;
            }
        }
    }
    /// The replicated weapon view with projectiles and drops at this frame's
    /// smoothed poses.
    pub fn weapons(&self) -> &WeaponView {
        &self.weapons
    }
    /// Package entities at this frame's smoothed poses, or `replicated`
    /// before the ghosts have seen the update at `tick`.
    pub fn entities_at<'a>(
        &'a self,
        tick: u64,
        replicated: &'a BTreeMap<u64, EntityInfo>,
    ) -> &'a BTreeMap<u64, EntityInfo> {
        if self.tick == Some(tick) {
            &self.entities
        } else {
            replicated
        }
    }
    /// The generic tracks, for other replicated bodies.
    pub fn tracks(&mut self) -> &mut Tracks {
        &mut self.tracks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn none(_: Vec3, _: Vec3) -> Option<Hit> {
        None
    }
    fn floor(from: Vec3, to: Vec3) -> Option<Hit> {
        (to.y < 0.0 && from.y >= 0.0).then(|| {
            let fraction = from.y / (from.y - to.y);
            Hit {
                position: from.lerp(to, fraction),
                normal: Vec3::Y,
                fraction,
                carry: None,
            }
        })
    }
    /// Two doorways linked as the sim's portal test places them, each a
    /// pair of openings back to back on one plane: in through the south
    /// side of the one at z = -4.25, out of the north side of the one at
    /// x = 10.25 turned a quarter, and back the other way.
    fn doorways() -> Passages {
        use bri_content::passage::Passage;
        use glam::{Affine3A, Vec2};
        let (a, b) = (Vec3::new(0.0, 1.5, -4.25), Vec3::new(10.25, 1.5, -4.0));
        let turn = Affine3A::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        let link = |from: Vec3, to: Vec3| {
            Affine3A::from_translation(to) * turn * Affine3A::from_translation(-from)
        };
        let opening = |centre, normal: Vec3, carry| Passage {
            brick: 1,
            centre,
            normal,
            u: Vec3::Y.cross(normal),
            v: Vec3::Y,
            half: Vec2::new(1.0, 1.5),
            carry,
        };
        Passages {
            list: vec![
                opening(a, Vec3::Z, link(a, b)),
                opening(b, Vec3::NEG_X, link(b, a)),
                opening(a, Vec3::NEG_Z, link(a, b).inverse()),
                opening(b, Vec3::X, link(b, a).inverse()),
            ],
            closed: vec![],
        }
    }
    #[test]
    fn a_ghost_flies_through_an_opening_as_the_host_does() {
        let passages = doorways();
        let carry = passages.list[0].carry;
        for speed in [3.0f32, 40.0, 200.0, 900.0] {
            for across in [-0.9f32, -0.3, 0.0, 0.55] {
                for before in [0.0005f32, 0.3, 2.0] {
                    // Slow arrows fall below the opening before they reach it.
                    let falls: &[f32] = if speed < 40.0 { &[0.0] } else { &[0.0, 9.81] };
                    for &gravity in falls {
                        let update = Update {
                            velocity: Vec3::new(0.1, 0.0, -1.0).normalize() * speed,
                            acceleration: Vec3::NEG_Y * gravity,
                            horizon: 1000.0,
                            ..Update::at(Vec3::new(across, 1.5, -4.25 + before))
                        };
                        // Half a unit past the opening.
                        let ticks = ((before + 0.5) / speed * 120.0).ceil() as f64;
                        let pose = simulate(&update, ticks, &mut |from, to| {
                            Hit::first(&passages, from, to, none)
                        });
                        let free = simulate(&update, ticks, &mut none);
                        let (position, velocity) = (
                            carry.transform_point3(free.position),
                            carry.transform_vector3(free.velocity),
                        );
                        let label = format!("{speed} at {across} from {before}, g {gravity}");
                        assert!(pose.position.distance(position) < 2e-3, "{label}: {pose:?}");
                        assert!(pose.velocity.distance(velocity) < 2e-3, "{label}: {pose:?}");
                    }
                }
            }
        }
    }
    #[test]
    fn simulation_matches_the_hosts_tick_integration() {
        let update = Update {
            velocity: Vec3::new(10.0, 12.0, 0.0),
            acceleration: Vec3::NEG_Y * 9.81,
            ..Update::at(Vec3::ZERO)
        };
        let (mut p, mut v) = (update.position, update.velocity);
        for _ in 0..12 {
            v += update.acceleration * TICK;
            p += v * TICK;
        }
        let pose = simulate(&update, 12.0, &mut none);
        assert!((pose.position - p).length() < 1e-4, "{pose:?} vs {p}");
    }
    #[test]
    fn simulation_stops_at_the_first_surface() {
        let update = Update {
            velocity: Vec3::new(0.0, -30.0, 0.0),
            ..Update::at(Vec3::Y)
        };
        let pose = simulate(&update, 12.0, &mut floor);
        assert!(pose.position.y.abs() < 1e-5 && pose.velocity == Vec3::ZERO);
        // A bouncing body reflects off it instead, losing speed.
        let bouncy = Update {
            bounce: Some(Bounce {
                elasticity: 0.5,
                friction: 0.0,
                rest_speed: 0.5,
            }),
            ..update
        };
        let pose = simulate(&bouncy, 12.0, &mut floor);
        assert!(
            pose.position.y > 0.0 && (pose.velocity.y - 15.0).abs() < 1e-3,
            "{pose:?}"
        );
    }
    #[test]
    fn a_disagreeing_update_warps_instead_of_popping() {
        let mut tracks = Tracks::default();
        let key = (Kind::Body, 1);
        tracks.arrived(0);
        let moving = Update {
            velocity: Vec3::X * 12.0,
            ..Update::at(Vec3::ZERO)
        };
        tracks.observe(key, 0, Mode::Simulated, moving, &mut none);
        tracks.advance(0.05);
        let before = tracks.pose(key, &mut none).unwrap().position;
        // The host says it stopped half a unit short.
        tracks.arrived(6);
        tracks.observe(
            key,
            6,
            Mode::Simulated,
            Update::at(Vec3::X * 0.1),
            &mut none,
        );
        let after = tracks.pose(key, &mut none).unwrap().position;
        assert!((after - before).length() < 1e-5);
        for _ in 0..30 {
            tracks.advance(1.0 / 60.0);
            tracks.pose(key, &mut none);
        }
        let settled = tracks.pose(key, &mut none).unwrap().position;
        assert!((settled - Vec3::X * 0.1).length() < 0.01, "{settled}");
    }
}
