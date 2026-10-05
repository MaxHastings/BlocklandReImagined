//! What a bot notices: brief glances, and a reaction time that depends on
//! what it was doing (`docs/architecture/bots.md`, "Noticing").
//!
//! A glance turns the bot's ordinary aim toward something salient for a
//! moment: a blast, a loud sound, someone who keeps looking straight at it,
//! something moving fast. Salience comes from engine data only (a blast's
//! radius, a sound's volume, a look's angle, a speed), never from content
//! names. Glances are rare (a chance by salience), short and cooled down,
//! and only an idle bot takes one: never with an enemy in sight, an
//! objective at hand, something held or a seat taken.
//!
//! A reaction is the delay between perceiving a new target or threat and
//! acting on it. It scales with how alert the bot was: shorter when already
//! fighting, longer when strolling or playing about, longer again when the
//! target was outside its view cone, where it also turns toward the target
//! more slowly until it has reacted. The same scale multiplies the kind's
//! starting aim error, which narrows as before while it tracks. The clock
//! starts when the target can be hurt: a spawn-protected one is watched,
//! not reacted to. Damage still interrupts at once (the chooser sees it);
//! only the return fire waits. Any other pause before acting on a change
//! (a chooser's tell) takes its length from [`Brain::switch_delay`].
//!
//! A hit from someone out of sight gives only a rough idea where from
//! ([`guess`]); the look holds until the reaction, and the exact spot comes
//! only from seeing them. An ally's warning is acted on after a short
//! seeded delay, at a spot a little off. The head turns as a person's does
//! ([`State::turn`]), and a strolling bot's look drifts a little
//! ([`drift`]). What it sees goes through the shared ray budget
//! (`sightlines`).
//!
//! The kind's `perception` (`bots.json`) holds seven numbers: `salience`
//! (scales every source's reach, 0 off), `glance_seconds`,
//! `cooldown_seconds`, `strength` (0 to 4, 1 shipped: scales the delay, aim
//! error, view-cone delay and turn cap, warning delay, turn overshoot,
//! drift and the steady aim error together; 0 plain), `relaxed_scale`,
//! `away_scale`, `view_degrees`. Reaches and rates come from engine data
//! (blast radius, sound volume, the kind's sight, turn rate and aim error,
//! the body's running speed) times fixed constants below. The RNG is the
//! bot's own seeded one; timers keyed to the tick use `cadence` with
//! perception's own salts.
use super::behaviour::Behaviour;
use super::*;
use crate::bot_kind::BotPerception;

/// Ticks between looks round for watchers, nearby players and fast
/// bodies, on the bot's own beat (`cadence`).
const POLL_TICKS: u64 = 12;
// Cadence names (`cadence::salt`) for perception's own timers.
const POLL_SALT: u64 = 101;
const HEAR_SALT: u64 = 102;
const HEAR_OFFSET_SALT: u64 = 103;
const DRIFT_SALT: u64 = 104;
/// Most stimuli one tick keeps for bots to notice.
const MAX_STIMULI: usize = 32;
// Fixed by how perception works, not per kind (the kind's `salience`
// scales every reach at once):
/// A stare is someone's look within this many degrees of the bot's eye...
const GAZE_DEGREES: f32 = 8.0;
/// ...held this long.
const GAZE_SECONDS: f32 = 1.5;
/// A blast is noticed out to this many units per unit of its radius...
const BLAST_REACH: f32 = 10.0;
/// ...a sound out to this many at full volume...
const SOUND_REACH: f32 = 12.0;
/// ...and a stare or fast motion out to this share of the kind's sight.
const SEEN_REACH: f32 = 0.3;
/// Someone close by draws the eye at most this likely a poll...
const NEAR: f32 = 0.03;
/// ...within this share of a stare's reach.
const NEAR_REACH: f32 = 0.5;
/// A body moving faster than this many times the bot's own running speed
/// is moving fast; it is fully salient at twice that.
const FAST: f32 = 2.0;
/// However long it tracks, its aim trails a target moving across its line
/// of sight by up to this many seconds of that motion, times `strength`: a
/// person's tracking lags a strafing target and keeps up with a still one.
/// The miss in units is then the same at any range. Measured with the
/// tuning lane's fair metric (a strafing, hopping target at 10 and 25
/// units, steady state): 0.5 s gives the Blockhead about 37% with the gun,
/// 28% with the bow and 18% with the rocket (1 s: 19/21/14%), inside the
/// 15-60% band; a still target is hit as before.
const STEADY_LAG: f32 = 0.5;
/// The most, in radians either way, that lag adds: a person tracking a
/// target crossing close by still keeps it near the crosshair. Above about
/// 0.3 a close strafer drew shots more than 25 degrees off it (the
/// gauntlet's off-target bar).
const STEADY_MOST: f32 = 0.3;
/// How far ahead, and how near its line, a shot's actual path is checked
/// for allies (`Session::bot_miss_spares_allies`).
const MISS_REACH: f32 = 60.0;
const MISS_CLEARANCE: f32 = 1.2;
/// Seconds the starting aim error takes to narrow (`bots.rs`' tracking).
const SETTLE_SECONDS: f32 = 2.0;
/// A reaction delay varies by up to this share either way.
const JITTER: f32 = 0.3;
/// An ally's warning is acted on this many seconds after it is heard
/// (seeded per ally and warning, scaled like a reaction)...
const HEAR_SECONDS: (f32, f32) = (0.25, 1.0);
/// ...at a spot up to this many units off where it was told.
const HEAR_OFFSET: f32 = 1.5;
/// Hurt by someone it cannot see, it knows the way the hit came from to
/// within this many degrees...
const HURT_DEGREES: f32 = 25.0;
/// ...and how far to within this share either way...
const HURT_BAND: f32 = 0.4;
/// ...but never nearer the truth than this many units: the exact spot
/// comes only from seeing them.
const HURT_MISS: f32 = 1.0;
// How a head turns (`State::turn`), in terms of the kind's plain turn
// rate, so a half turn takes about as long as the plain linear one:
/// its top speed, this many times the plain rate...
const TURN_PEAK: f32 = 1.3;
/// ...reached at this acceleration, per plain rate squared (with the peak,
/// a half turn is as quick as the plain one)...
const TURN_ACCEL: f32 = 1.8;
/// ...braking late by up to this share at top speed, so a flick
/// overshoots a little and settles back...
const TURN_OVERSHOOT: f32 = 0.04;
/// ...and resting once within this many radians at a crawl.
const TURN_REST: f32 = 0.002;
/// An idle bot's look drifts up to this many degrees either way...
const DRIFT_DEGREES: f32 = 7.0;
/// ...over two slow swings of these many seconds.
const DRIFT_SECONDS: (f32, f32) = (7.0, 2.9);

/// The bots' seeded generator: the next value in [0, 1).
pub(super) fn draw(rng: &mut u64) -> f32 {
    *rng = rng
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*rng >> 40) as f32 / (1u64 << 24) as f32
}
/// Where a bot at `from`, hurt by someone at `at` it cannot see, thinks
/// they are: the incoming direction turned by up to `HURT_DEGREES`, the
/// distance off by up to `HURT_BAND`, and at least `HURT_MISS` from `at`.
pub(super) fn guess(from: Vec3, at: Vec3, rng: &mut u64) -> Vec3 {
    let to = at - from;
    let distance = to.length();
    if distance < 0.01 {
        return at;
    }
    let angle = (draw(rng) * 2.0 - 1.0) * HURT_DEGREES.to_radians();
    let length = distance * (1.0 + HURT_BAND * (draw(rng) * 2.0 - 1.0));
    let (sin, cos) = angle.sin_cos();
    let way = to / distance;
    let way = Vec3::new(way.x * cos - way.z * sin, way.y, way.x * sin + way.z * cos);
    let guess = from + way * length;
    let miss = guess - at;
    if miss.length() >= HURT_MISS {
        return guess;
    }
    let off = if miss.length() > 1e-3 {
        miss.normalize()
    } else {
        way
    };
    at + off * HURT_MISS
}
fn ticks(seconds: f32) -> u64 {
    (seconds.max(0.0) * 120.0) as u64
}

/// Something that happened where a bot may notice it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::session) struct Stimulus {
    at: Vec3,
    source: Source,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Source {
    Blast {
        radius: f32,
    },
    Sound {
        volume: f32,
    },
    Gaze,
    /// Someone close by in plain view, of any team.
    Near,
    /// How much faster than fast, 0 to 1.
    Motion {
        strength: f32,
    },
}
impl Source {
    fn name(self) -> &'static str {
        match self {
            Self::Blast { .. } => "glance: blast",
            Self::Sound { .. } => "glance: sound",
            Self::Gaze => "glance: watched",
            Self::Near => "glance: someone near",
            Self::Motion { .. } => "glance: fast motion",
        }
    }
}
impl Stimulus {
    pub(in crate::session) fn blast(at: Vec3, radius: f32) -> Self {
        Self {
            at,
            source: Source::Blast { radius },
        }
    }
    pub(in crate::session) fn sound(at: Vec3, volume: f32) -> Self {
        Self {
            at,
            source: Source::Sound { volume },
        }
    }
    /// The chance it draws the eye of a bot at `from` that sees `sight`
    /// units: 1 at the source, falling off to 0 at its reach, which the
    /// kind's `salience` scales.
    fn salience(&self, p: &BotPerception, sight: f32, from: Vec3) -> f32 {
        let seen = sight * SEEN_REACH;
        let (strength, reach) = match self.source {
            Source::Blast { radius } => (1.0, radius.max(0.0) * BLAST_REACH),
            Source::Sound { volume } => (1.0, volume.clamp(0.0, 1.0) * SOUND_REACH),
            Source::Gaze => (1.0, seen),
            Source::Near => (NEAR, seen * NEAR_REACH),
            Source::Motion { strength } => (strength, seen),
        };
        let reach = reach * p.salience;
        let distance = from.distance(self.at);
        if !(strength > 0.0 && reach > 0.0 && distance < reach) {
            return 0.0;
        }
        strength * (1.0 - distance / reach)
    }
}
impl Bots {
    /// Something happened a bot may notice; bots look next tick.
    pub(in crate::session) fn notice(&mut self, stimulus: Stimulus) {
        if !self.brains.is_empty() && self.stimuli.len() < MAX_STIMULI && stimulus.at.is_finite() {
            self.stimuli.push(stimulus);
        }
    }
}

/// How alert a bot was when something new came up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Alertness {
    /// Already fighting or hunting.
    Combat,
    Ordinary,
    /// Strolling or playing about.
    Relaxed,
}
impl Alertness {
    /// From what it was doing. A flavour or play option belongs with
    /// Wander here.
    pub(super) fn of(behaviour: Behaviour) -> Self {
        match behaviour {
            Behaviour::Fight | Behaviour::Chase | Behaviour::Fly | Behaviour::Search => {
                Self::Combat
            }
            Behaviour::Wander | Behaviour::Interact => Self::Relaxed,
            _ => Self::Ordinary,
        }
    }
    fn name(self, away: bool) -> &'static str {
        match (self, away) {
            (Self::Combat, false) => "reacting: in combat",
            (Self::Combat, true) => "reacting: in combat, from behind",
            (Self::Ordinary, false) => "reacting",
            (Self::Ordinary, true) => "reacting: from behind",
            (Self::Relaxed, false) => "reacting: relaxed",
            (Self::Relaxed, true) => "reacting: relaxed, from behind",
        }
    }
}
/// Only an idle bot glances: one strolling about or walking home. Only a
/// strolling one looks round at someone merely near (`Source::Near`):
/// walking home is going somewhere (in a ball game, back to its place for
/// the next play), and those looks held 2v2 soccer bots' heads off their
/// way (every one of 293 glances in a seed-2 match was there).
pub(super) fn may_glance(behaviour: Behaviour) -> bool {
    matches!(behaviour, Behaviour::Wander | Behaviour::Return)
}
/// How much slower than plain it reacts (and how much wider its first aim
/// errs): by alertness, and more from outside its view cone, blended by
/// the kind's `strength`.
fn scale(p: &BotPerception, alertness: Alertness, away: bool) -> f32 {
    let scale = match alertness {
        // Fighting or hunting: the kind's plain numbers.
        Alertness::Combat => 1.0,
        Alertness::Ordinary => 1.0,
        Alertness::Relaxed => p.relaxed_scale,
    } * if away { p.away_scale } else { 1.0 };
    (1.0 + p.strength * (scale - 1.0)).max(0.0)
}
/// Ticks of delay before acting on something new: the kind's plain
/// `reaction_seconds` scaled by alertness and view, varied by `JITTER`.
/// With the kind's `strength` 0, exactly `reaction_seconds`, drawing
/// nothing. Any pause before acting on a change uses this.
pub(super) fn delay_ticks(
    p: &BotPerception,
    reaction_seconds: f32,
    alertness: Alertness,
    away: bool,
    rng: &mut u64,
) -> u64 {
    if p.strength <= 0.0 {
        return ticks(reaction_seconds);
    }
    let jitter = 1.0 + p.strength * JITTER * (draw(rng) * 2.0 - 1.0);
    ticks(reaction_seconds * scale(p, alertness, away) * jitter)
}

/// What the bot noticed last and why, for the brain readout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BotNotice {
    /// `glance: blast`, `reacting: relaxed` and so on.
    pub why: &'static str,
    pub since: u64,
    /// The glance's end, or when the reaction lets it act.
    pub until: u64,
}
#[derive(Clone, Copy, Debug)]
struct Glance {
    at: Vec3,
    until: u64,
}
#[derive(Clone, Copy, Debug)]
struct Reaction {
    subject: OwnerId,
    ready: u64,
    /// Its starting aim error's multiplier.
    scale: f32,
    /// Its turn rate's multiplier until `ready`.
    turn: f32,
    /// Started by a hit from someone out of sight: it holds its look
    /// until `ready`, then turns.
    unseen: bool,
}
/// A brain's noticing.
#[derive(Clone, Debug, Default)]
pub(super) struct State {
    glance: Option<Glance>,
    next_glance: u64,
    /// Who has been looking straight at it, since when.
    watcher: Option<(OwnerId, u64)>,
    reaction: Option<Reaction>,
    /// An ally's warning, acted on from its tick.
    heard: Option<(u64, Knowledge)>,
    /// How fast its head turns now, radians a second (`State::turn`).
    turn_rate: f32,
    pub(super) why: Option<BotNotice>,
}
impl State {
    /// Where it glances this tick, if anywhere. An ongoing glance holds;
    /// otherwise, off cooldown, the most salient of `stimuli` may start
    /// one, by a chance its salience gives.
    #[allow(clippy::too_many_arguments)]
    fn glance(
        &mut self,
        p: &BotPerception,
        sight: f32,
        tick: u64,
        eye: Vec3,
        eligible: bool,
        stimuli: impl IntoIterator<Item = Stimulus>,
        rng: &mut u64,
    ) -> Option<Vec3> {
        if !eligible {
            self.glance = None;
            return None;
        }
        if let Some(glance) = self.glance {
            if tick < glance.until {
                return Some(glance.at);
            }
            self.glance = None;
        }
        if tick < self.next_glance {
            return None;
        }
        let (salience, best) = stimuli
            .into_iter()
            .map(|s| (s.salience(p, sight, eye), s))
            .filter(|(salience, s)| *salience > 0.0 && s.at.distance(eye) > 0.5)
            .max_by(|a, b| a.0.total_cmp(&b.0))?;
        if draw(rng) >= salience {
            return None;
        }
        let until = tick + ticks(p.glance_seconds * (0.75 + 0.5 * draw(rng))).max(1);
        self.glance = Some(Glance { at: best.at, until });
        self.next_glance = until + ticks(p.cooldown_seconds);
        self.why = Some(BotNotice {
            why: best.source.name(),
            since: tick,
            until,
        });
        Some(best.at)
    }
    /// Someone at `eye` looks straight at it now (or no one): once they
    /// have for `GAZE_SECONDS`, it is a stimulus, and the stare counts
    /// again from there.
    fn watched(&mut self, tick: u64, by: Option<(OwnerId, Vec3)>) -> Option<Stimulus> {
        let Some((who, eye)) = by else {
            self.watcher = None;
            return None;
        };
        let since = match self.watcher {
            Some((watcher, since)) if watcher == who => since,
            _ => {
                self.watcher = Some((who, tick));
                tick
            }
        };
        (tick - since >= ticks(GAZE_SECONDS)).then(|| {
            self.watcher = Some((who, tick));
            Stimulus {
                at: eye,
                source: Source::Gaze,
            }
        })
    }
    /// A new target or threat: the tick it may act on it. A reaction
    /// already pending for the same subject (hurt, then seen) stands.
    #[allow(clippy::too_many_arguments)]
    fn react(
        &mut self,
        p: &BotPerception,
        reaction_seconds: f32,
        subject: OwnerId,
        tick: u64,
        alertness: Alertness,
        away: bool,
        rng: &mut u64,
    ) -> u64 {
        if let Some(r) = self
            .reaction
            .filter(|r| r.subject == subject && r.ready > tick)
        {
            return r.ready;
        }
        let ready = tick + delay_ticks(p, reaction_seconds, alertness, away, rng);
        self.reaction = Some(Reaction {
            subject,
            ready,
            scale: scale(p, alertness, away),
            turn: if away {
                1.0 / scale(p, Alertness::Ordinary, true).max(1.0)
            } else {
                1.0
            },
            unseen: false,
        });
        if p.strength > 0.0 {
            self.why = Some(BotNotice {
                why: alertness.name(away),
                since: tick,
                until: ready,
            });
        }
        ready
    }
    /// It may not hurt `subject` yet (spawn protection): its reaction
    /// waits for when it can.
    fn withhold(&mut self, subject: OwnerId) {
        if self.reaction.is_some_and(|r| r.subject == subject) {
            self.reaction = None;
        }
    }
    /// Whether its reaction to `subject` has passed; `None` when it has
    /// none for them.
    pub(super) fn acted(&self, subject: OwnerId, tick: u64) -> Option<bool> {
        self.reaction
            .filter(|r| r.subject == subject)
            .map(|r| tick >= r.ready)
    }
    /// What its aim error is multiplied by after tracking `subject` for
    /// `tracked` seconds: the reaction's scale, narrowing to 1.
    pub(super) fn aim_scale(&self, subject: OwnerId, tracked: f32) -> f32 {
        self.reaction
            .filter(|r| r.subject == subject)
            .map_or(1.0, |r| {
                1.0 + (r.scale - 1.0) * (1.0 - tracked / SETTLE_SECONDS).max(0.0)
            })
    }
    /// Its yaw one tick on from `yaw` toward `aim`, turning as a person
    /// does: speeding up into a big turn and easing out of it, a fast flick
    /// overshooting a little and settling back, so a half turn takes about
    /// the plain time but is never at one rate. `step` is the plain linear
    /// turn's limit this tick (0 holds the look). The kind's `strength`
    /// scales the overshoot; at 0 the turn is the plain linear one.
    pub(super) fn turn(&mut self, p: &BotPerception, yaw: f32, aim: f32, step: f32) -> f32 {
        if p.strength <= 0.0 || step <= 0.0 {
            self.turn_rate = 0.0;
            return super::turn(yaw, aim, step);
        }
        let dt = 1.0 / 120.0;
        let plain = step / dt;
        let peak = TURN_PEAK * plain;
        let accel = TURN_ACCEL * plain * plain;
        let error = wrap(aim - yaw);
        if error.abs() < TURN_REST && self.turn_rate.abs() <= accel * dt {
            self.turn_rate = 0.0;
            return aim;
        }
        // The speed that would just stop on the aim, braking late when fast.
        let late = 1.0 + TURN_OVERSHOOT * p.strength * (self.turn_rate.abs() / peak).min(1.0);
        let wanted = error.signum() * (2.0 * accel * error.abs()).sqrt().min(peak / late) * late;
        self.turn_rate += (wanted - self.turn_rate).clamp(-accel * dt, accel * dt);
        wrap(yaw + self.turn_rate * dt)
    }
    /// Hit by someone out of sight and not yet reacted: its look holds.
    pub(super) fn startled(&self, tick: u64) -> bool {
        self.reaction.is_some_and(|r| r.unseen && tick < r.ready)
    }
    /// An ally's warning `k` reaches `bot`: it acts on it a seeded
    /// `HEAR_SECONDS` later, scaled like a reaction, at a spot up to
    /// `HEAR_OFFSET` off. A newer warning while one is pending updates
    /// what it knows but not when it acts (a warner repeats itself). With
    /// the kind's `strength` 0, at once and exactly.
    fn hear(
        &mut self,
        p: &BotPerception,
        k: Knowledge,
        alertness: Alertness,
        bot: OwnerId,
        tick: u64,
    ) {
        if self
            .heard
            .is_some_and(|(_, old)| old.observed >= k.observed)
        {
            return;
        }
        let (lo, hi) = HEAR_SECONDS;
        let seconds =
            cadence::spread(bot, HEAR_SALT, k.observed, lo, hi) * scale(p, alertness, false);
        let angle = cadence::spread(
            bot,
            HEAR_OFFSET_SALT,
            k.observed,
            0.0,
            std::f32::consts::TAU,
        );
        let off = cadence::spread(bot, HEAR_OFFSET_SALT ^ 1, k.observed, 0.0, HEAR_OFFSET);
        let k = Knowledge {
            at: k.at + Vec3::new(angle.cos(), 0.0, angle.sin()) * off * p.strength,
            ..k
        };
        let due = self
            .heard
            .map_or(tick + ticks(seconds * p.strength), |(due, _)| due);
        self.heard = Some((due, k));
    }
    /// The warning it acts on now, if one is due.
    pub(super) fn heard(&mut self, tick: u64) -> Option<Knowledge> {
        let (_, k) = self.heard.filter(|(due, _)| tick >= *due)?;
        self.heard = None;
        Some(k)
    }
    /// What its turn rate toward `subject` is multiplied by now: slower
    /// toward one from outside its view cone, until it has reacted.
    pub(super) fn turn_scale(&self, subject: OwnerId, tick: u64) -> f32 {
        self.reaction
            .filter(|r| r.subject == subject && tick < r.ready)
            .map_or(1.0, |r| r.turn)
    }
}

/// How far, in radians either way, its aim keeps erring however long it
/// tracks a target `distance` away moving `across` units a second across
/// its line of sight (`STEADY_LAG`); 0 at `strength` 0.
pub(super) fn steady_error(p: &BotPerception, across: f32, distance: f32) -> f32 {
    (STEADY_LAG * p.strength * across.max(0.0) / distance.max(1.0)).min(STEADY_MOST)
}
/// Where a bot shooting along `direction` believes it aims: without its
/// aim `error` (yaw, pitch). The fire gate judges a shot by this, so the
/// error is a real miss rather than a shot held back; at `strength` 0 the
/// gate judges the actual direction, as before.
pub(super) fn believed(p: &BotPerception, direction: Vec3, error: (f32, f32)) -> Vec3 {
    if p.strength <= 0.0 || !direction.is_finite() || direction.length_squared() < 1e-6 {
        return direction;
    }
    let d = direction.normalize();
    let yaw = d.x.atan2(-d.z) - error.0;
    let pitch = d.y.clamp(-1.0, 1.0).asin() - error.1;
    Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    )
}
/// How far an idle bot's look has drifted off its way at `tick`, radians:
/// two slow swings on the bot's own phase, scaled by the kind's
/// `strength`.
pub(super) fn drift(p: &BotPerception, bot: OwnerId, tick: u64) -> f32 {
    let t = (tick + cadence::bot_phase(bot, DRIFT_SALT)) as f32 / 120.0;
    let (slow, quick) = DRIFT_SECONDS;
    let tau = std::f32::consts::TAU;
    let swing = 0.7 * (t * tau / slow).sin() + 0.3 * (t * tau / quick).sin();
    DRIFT_DEGREES.to_radians() * p.strength * swing
}

impl Brain {
    /// It perceives `subject` at `at`, standing at `feet`; `fresh` when
    /// they were not its target a tick ago. While it may not hurt them
    /// (`damageable` false) no reaction runs. Otherwise a fresh one, or one
    /// it has no reaction for, starts its reaction (`State::react`) from how
    /// alert it was and whether they were outside its view cone. A started
    /// reaction draws its aim error afresh.
    pub(super) fn perceive(
        &mut self,
        subject: OwnerId,
        at: Vec3,
        feet: Vec3,
        tick: u64,
        fresh: bool,
        damageable: bool,
    ) {
        if !damageable {
            self.perception.withhold(subject);
            return;
        }
        if !fresh && self.perception.acted(subject, tick).is_some() {
            return;
        }
        let to = flat(at - feet);
        let half_cone = (self.kind.perception.view_degrees * 0.5).to_radians();
        let away = to.length() > 0.01 && wrap(yaw_to(to) - self.yaw).abs() > half_cone;
        let alertness = self.alertness();
        let Self {
            perception,
            kind,
            rng,
            ..
        } = self;
        perception.react(
            &kind.perception,
            kind.reaction_seconds,
            subject,
            tick,
            alertness,
            away,
            rng,
        );
    }
    /// It was hurt by `k.subject`, not the target it is fighting: with the
    /// reaction model on, its return fire waits a reaction.
    pub(super) fn hurt_by(&mut self, k: &Knowledge, feet: Vec3, tick: u64) {
        if self.kind.perception.strength > 0.0 && self.target != Some(k.subject) {
            self.perceive(k.subject, k.at, feet, tick, true, true);
            if let Some(r) = self.perception.reaction.as_mut().filter(|r| r.ready > tick) {
                r.unseen = true;
            }
        }
    }
    /// An ally's warning reached it (`State::hear`).
    pub(super) fn hear(&mut self, k: Knowledge, bot: OwnerId, tick: u64) {
        let alertness = self.alertness();
        self.perception
            .hear(&self.kind.perception, k, alertness, bot, tick);
    }
    /// How alert it is now: a bot goofing (`surprise`) is relaxed,
    /// whatever its behaviour.
    pub(super) fn alertness(&self) -> Alertness {
        if self.surprise.flavour().is_some() {
            Alertness::Relaxed
        } else {
            Alertness::of(self.behaviour)
        }
    }
    /// Ticks to pause before acting on a change of mind (a chooser's tell):
    /// the same reaction delay, by how alert it is now.
    #[allow(dead_code)]
    pub(super) fn switch_delay(&mut self) -> u64 {
        let alertness = self.alertness();
        delay_ticks(
            &self.kind.perception,
            self.kind.reaction_seconds,
            alertness,
            false,
            &mut self.rng,
        )
    }
}
impl Session {
    /// Whether a shot from `bot`'s eye along `direction` (where its aim
    /// actually points, error and all) passes no living ally's body within
    /// `MISS_REACH`: the fire gate judges the shot where the bot believes
    /// it aims, and a miss must not go into its own side.
    pub(super) fn bot_miss_spares_allies(&self, bot: OwnerId, direction: Vec3) -> bool {
        let Some(me) = self.peers.get(&bot) else {
            return false;
        };
        let (origin, direction) = (me.player.eye(), direction.normalize_or_zero());
        !self.peers.iter().any(|(o, p)| {
            if *o == bot || !p.combat.alive || !self.bot_allies(bot, *o) {
                return false;
            }
            let centre = Vec3::from(p.player.state().feet) + Vec3::Y;
            let along = (centre - origin).dot(direction);
            (0.0..MISS_REACH).contains(&along)
                && (centre - origin - direction * along).length() < MISS_CLEARANCE
        })
    }
    /// Where `bot` glances this tick (`State::glance`). `eligible` is the
    /// caller's: idle, nothing in sight, no objective, nothing held, not
    /// seated or driving.
    pub(super) fn bot_glance(
        &mut self,
        bot: OwnerId,
        tick: u64,
        eye: Vec3,
        eligible: bool,
        strolling: bool,
    ) -> Option<Vec3> {
        let kind = &self.bots.brains.get(&bot)?.kind;
        let (p, sight) = (kind.perception.clone(), kind.sight);
        let reach = sight * SEEN_REACH * p.salience;
        let fast = FAST * self.peers.get(&bot)?.player.tuning().forward.max(0.1);
        let mut stimuli = self.bots.stimuli.clone();
        let poll = eligible && reach > 0.0 && cadence::beat(bot, POLL_SALT, tick, POLL_TICKS);
        let (watcher, near) = if poll {
            self.bot_watcher(bot, eye, reach)
        } else {
            (None, Vec::new())
        };
        if strolling {
            stimuli.extend(near);
        }
        if poll {
            stimuli.extend(
                self.bots
                    .objects
                    .iter()
                    .filter(|v| !v.destroyed)
                    .map(|v| {
                        let at = Vec3::from(v.transform.position);
                        (v.id.0, at, Vec3::from(v.velocity))
                    })
                    .filter(|(id, at, velocity)| {
                        let subject = Some(super::SightSubject::Vehicle(*id));
                        let urgency = super::SightUrgency::Ordinary;
                        velocity.length() > fast
                            && at.distance(eye) < reach
                            && self
                                .bot_sees(bot, subject, eye, *at, reach, urgency)
                                .is_some()
                    })
                    .map(|(_, at, velocity)| Stimulus {
                        at,
                        source: Source::Motion {
                            strength: (velocity.length() / fast - 1.0).min(1.0),
                        },
                    }),
            );
        }
        let brain = self.bots.brains.get_mut(&bot)?;
        if !eligible {
            brain.perception.watcher = None;
        } else if poll {
            stimuli.extend(brain.perception.watched(tick, watcher));
        }
        let Brain {
            perception, rng, ..
        } = brain;
        perception.glance(&p, sight, tick, eye, eligible, stimuli, rng)
    }
    /// Players in plain view within `range` of `bot`'s eye (through the
    /// shared sight budget): the nearest whose look points within
    /// `GAZE_DEGREES` of that eye (the watcher), and every one as a nearby
    /// presence, of any team.
    fn bot_watcher(
        &self,
        bot: OwnerId,
        eye: Vec3,
        range: f32,
    ) -> (Option<(OwnerId, Vec3)>, Vec<Stimulus>) {
        let cone = GAZE_DEGREES.to_radians().cos();
        let mut watcher: Option<(OwnerId, Vec3, f32)> = None;
        let mut near = Vec::new();
        for (owner, peer) in &self.peers {
            if *owner == bot || !peer.combat.alive {
                continue;
            }
            let state = peer.player.state();
            let from = peer.player.eye();
            let to = eye - from;
            let distance = to.length();
            if !(distance > 0.5 && distance < range) {
                continue;
            }
            let urgency = super::SightUrgency::Ordinary;
            if self
                .bot_sees_player(bot, *owner, eye, range, urgency)
                .is_none()
            {
                continue;
            }
            let look = Vec3::new(
                state.yaw.sin() * state.pitch.cos(),
                state.pitch.sin(),
                -state.yaw.cos() * state.pitch.cos(),
            );
            if look.dot(to / distance) >= cone && watcher.is_none_or(|w| distance < w.2) {
                watcher = Some((*owner, from, distance));
            }
            near.push(Stimulus {
                at: from,
                source: Source::Near,
            });
        }
        (watcher.map(|(owner, from, _)| (owner, from)), near)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> BotPerception {
        BotPerception::default()
    }
    /// The Blockhead's sight: a stare or fast motion reaches 24 units.
    const SIGHT: f32 = 80.0;
    fn blast_at(x: f32) -> Stimulus {
        Stimulus::blast(Vec3::new(x, 1.0, 0.0), 4.0)
    }
    const EYE: Vec3 = Vec3::new(0.0, 1.0, 0.0);

    #[test]
    fn a_salient_blast_in_range_draws_a_glance_only_from_an_idle_bot() {
        let p = on();
        let mut rng = 7;
        let mut idle = State::default();
        assert_eq!(
            idle.glance(&p, SIGHT, 100, EYE, true, [blast_at(1.0)], &mut rng),
            Some(blast_at(1.0).at)
        );
        assert_eq!(idle.why.unwrap().why, "glance: blast");
        // It holds for the glance, then lets go.
        let until = idle.why.unwrap().until;
        assert!(until > 100 && until <= 100 + ticks(p.glance_seconds * 1.25) + 1);
        assert!(
            idle.glance(&p, SIGHT, until - 1, EYE, true, [], &mut rng)
                .is_some()
        );
        assert!(
            idle.glance(&p, SIGHT, until, EYE, true, [], &mut rng)
                .is_none()
        );
        // Carrying an objective, in combat aim or driving, the caller says
        // not eligible: no glance, and an ongoing one ends.
        let mut busy = State::default();
        assert!(
            busy.glance(&p, SIGHT, 100, EYE, false, [blast_at(1.0)], &mut rng)
                .is_none()
        );
        assert!(busy.why.is_none());
        let mut interrupted = State::default();
        let mut rng = 7;
        assert!(
            interrupted
                .glance(&p, SIGHT, 100, EYE, true, [blast_at(1.0)], &mut rng)
                .is_some()
        );
        assert!(
            interrupted
                .glance(&p, SIGHT, 101, EYE, false, [], &mut rng)
                .is_none()
        );
        assert!(
            interrupted
                .glance(&p, SIGHT, 102, EYE, true, [], &mut rng)
                .is_none()
        );
        // Behaviours: only strolling or walking home glances.
        assert!(may_glance(Behaviour::Wander) && may_glance(Behaviour::Return));
        for b in [
            Behaviour::Objective,
            Behaviour::Carry,
            Behaviour::Fight,
            Behaviour::Chase,
            Behaviour::Search,
            Behaviour::Interact,
        ] {
            assert!(!may_glance(b), "{b:?}");
        }
    }

    #[test]
    fn salience_falls_off_to_each_sources_reach() {
        let p = on();
        // Radius 4 reaches 4 * BLAST_REACH = 40 units at salience 1.
        assert!(blast_at(20.0).salience(&p, SIGHT, EYE) > 0.49);
        assert_eq!(blast_at(40.0).salience(&p, SIGHT, EYE), 0.0);
        let near = Vec3::new(1.0, 1.0, 0.0);
        assert_eq!(Stimulus::sound(near, 0.0).salience(&p, SIGHT, EYE), 0.0);
        assert!(Stimulus::sound(near, 1.0).salience(&p, SIGHT, EYE) > 0.9);
        let mut rng = 7;
        let mut s = State::default();
        assert!(
            s.glance(&p, SIGHT, 1, EYE, true, [blast_at(41.0)], &mut rng)
                .is_none()
        );
    }

    #[test]
    fn glances_keep_their_cooldown() {
        let p = on();
        let mut rng = 11;
        let mut s = State::default();
        s.glance(&p, SIGHT, 0, EYE, true, [blast_at(0.6)], &mut rng)
            .unwrap();
        let until = s.why.unwrap().until;
        let cooled = until + ticks(p.cooldown_seconds);
        for tick in until..cooled {
            assert!(
                s.glance(&p, SIGHT, tick, EYE, true, [blast_at(0.6)], &mut rng)
                    .is_none(),
                "{tick}"
            );
        }
        let after = (cooled..cooled + 20)
            .find(|tick| {
                s.glance(&p, SIGHT, *tick, EYE, true, [blast_at(0.6)], &mut rng)
                    .is_some()
            })
            .expect("a glance once cooled down");
        assert!(after >= cooled);
    }

    #[test]
    fn a_long_stare_is_noticed_and_counts_again_after() {
        let p = on();
        let watcher = Some((9, Vec3::new(1.0, 1.0, 0.0)));
        let mut s = State::default();
        let stare = ticks(GAZE_SECONDS);
        assert!(s.watched(0, watcher).is_none());
        assert!(s.watched(stare - 1, watcher).is_none());
        let seen = s.watched(stare, watcher).unwrap();
        assert_eq!(seen.source, Source::Gaze);
        assert!(s.watched(stare + 1, watcher).is_none());
        // Looking away resets the stare; someone else starts their own.
        assert!(s.watched(stare + 2, None).is_none());
        assert!(s.watched(2 * stare + 1, watcher).is_none());
        assert!(s.watched(2 * stare + 1, Some((8, EYE))).is_none());
        let mut rng = 3;
        assert!(
            s.glance(&p, SIGHT, 0, EYE, true, [seen], &mut rng)
                .is_some()
        );
        assert_eq!(s.why.unwrap().why, "glance: watched");
    }

    #[test]
    fn fast_motion_needs_speed_over_its_threshold() {
        let p = on();
        let at = Vec3::new(3.0, 1.0, 0.0);
        let moving = |strength| Stimulus {
            at,
            source: Source::Motion { strength },
        };
        assert_eq!(moving(0.0).salience(&p, SIGHT, EYE), 0.0);
        assert!(moving(1.0).salience(&p, SIGHT, EYE) > 0.8);
        // Out past its share of the kind's sight.
        let far = Stimulus {
            at: Vec3::new(25.0, 1.0, 0.0),
            source: Source::Motion { strength: 1.0 },
        };
        assert_eq!(far.salience(&p, SIGHT, EYE), 0.0);
    }

    #[test]
    fn a_goofing_bot_reacts_slower_and_errs_wider_than_one_in_combat() {
        let p = on();
        for seed in 1..50u64 {
            let (mut relaxed, mut fighting) = (State::default(), State::default());
            let (mut a, mut b) = (seed, seed);
            let slow = relaxed.react(&p, 0.35, 5, 1000, Alertness::Relaxed, false, &mut a);
            let quick = fighting.react(&p, 0.35, 5, 1000, Alertness::Combat, false, &mut b);
            assert!(slow > quick, "seed {seed}: {slow} vs {quick}");
            assert!(relaxed.aim_scale(5, 0.0) > 1.0 && fighting.aim_scale(5, 0.0) == 1.0);
            // From outside its view cone, slower again, and turning slower
            // until it has reacted.
            let mut c = seed;
            let mut behind = State::default();
            let away = behind.react(&p, 0.35, 5, 1000, Alertness::Relaxed, true, &mut c);
            assert!(away > slow, "seed {seed}");
            assert!(behind.turn_scale(5, 1000) < 1.0);
            assert_eq!(behind.turn_scale(5, away), 1.0);
            assert_eq!(relaxed.turn_scale(5, 1000), 1.0);
        }
        // The wider error narrows, and only on that subject.
        let mut s = State::default();
        let mut rng = 1;
        s.react(&p, 0.35, 5, 0, Alertness::Relaxed, false, &mut rng);
        assert!(s.aim_scale(5, 0.0) > 1.5);
        assert_eq!(s.aim_scale(5, SETTLE_SECONDS), 1.0);
        assert_eq!(s.aim_scale(6, 0.0), 1.0);
        assert_eq!(s.why.unwrap().why, "reacting: relaxed");
    }

    #[test]
    fn delays_vary_by_seed() {
        let p = on();
        let ready: std::collections::BTreeSet<u64> = (1..20u64)
            .map(|mut seed| delay_ticks(&p, 0.35, Alertness::Ordinary, false, &mut seed))
            .collect();
        assert!(ready.len() > 3, "{ready:?}");
    }

    #[test]
    fn hurt_then_seen_keeps_the_pending_reaction() {
        let p = on();
        let mut rng = 2;
        let mut s = State::default();
        let ready = s.react(&p, 0.35, 5, 100, Alertness::Relaxed, true, &mut rng);
        assert_eq!(s.acted(5, ready - 1), Some(false));
        // Seen a tick later: the same pending reaction, not a new one.
        assert_eq!(
            s.react(&p, 0.35, 5, 101, Alertness::Combat, false, &mut rng),
            ready
        );
        assert_eq!(s.acted(5, ready), Some(true));
        assert_eq!(s.acted(6, ready), None);
    }

    #[test]
    fn a_protected_target_restarts_the_reaction_when_it_can_be_hurt() {
        let p = on();
        let mut rng = 4;
        let mut s = State::default();
        s.react(&p, 0.35, 5, 0, Alertness::Combat, false, &mut rng);
        s.withhold(5);
        assert_eq!(s.acted(5, 10_000), None);
        let ready = s.react(&p, 0.35, 5, 600, Alertness::Combat, false, &mut rng);
        assert!(ready > 600);
        assert_eq!(s.acted(5, 600), Some(false));
    }

    #[test]
    fn salience_and_alertness_at_zero_turn_noticing_off() {
        let p = BotPerception {
            salience: 0.0,
            strength: 0.0,
            ..Default::default()
        };
        let mut rng = 5;
        let mut s = State::default();
        for (tick, stimulus) in [
            blast_at(1.0),
            Stimulus::sound(EYE + Vec3::X, 1.0),
            Stimulus {
                at: EYE + Vec3::X,
                source: Source::Gaze,
            },
            Stimulus {
                at: EYE + Vec3::X,
                source: Source::Motion { strength: 1.0 },
            },
        ]
        .into_iter()
        .enumerate()
        {
            assert!(
                s.glance(&p, SIGHT, tick as u64, EYE, true, [stimulus], &mut rng)
                    .is_none()
            );
        }
        let before = rng;
        // The plain reaction: exactly `reaction_seconds`, nothing drawn,
        // no wider aim, no slower turn, nothing in the readout.
        assert_eq!(
            s.react(&p, 0.35, 5, 0, Alertness::Relaxed, true, &mut rng),
            ticks(0.35)
        );
        assert_eq!(rng, before);
        assert_eq!(s.aim_scale(5, 0.0), 1.0);
        assert_eq!(s.turn_scale(5, 0), 1.0);
        assert!(s.why.is_none());
    }

    #[test]
    fn alertness_follows_what_it_was_doing() {
        assert_eq!(Alertness::of(Behaviour::Fight), Alertness::Combat);
        assert_eq!(Alertness::of(Behaviour::Search), Alertness::Combat);
        assert_eq!(Alertness::of(Behaviour::Wander), Alertness::Relaxed);
        assert_eq!(Alertness::of(Behaviour::Interact), Alertness::Relaxed);
        assert_eq!(Alertness::of(Behaviour::Objective), Alertness::Ordinary);
    }

    #[test]
    fn an_unseen_attacker_is_placed_roughly_and_never_exactly() {
        let from = Vec3::new(0.0, 0.0, 0.0);
        let at = Vec3::new(0.0, 0.0, -30.0);
        let mut rng = 5;
        let mut spread = 0.0f32;
        for _ in 0..200 {
            let g = guess(from, at, &mut rng);
            assert!(g.distance(at) >= HURT_MISS - 1e-4, "{g}");
            // Roughly the way the hit came from, roughly as far.
            let angle = flat(g - from).angle_between(flat(at - from)).to_degrees();
            assert!(angle <= HURT_DEGREES + 2.5, "{angle}");
            let d = g.distance(from) / 30.0;
            assert!(
                (1.0 - HURT_BAND - 0.05..=1.0 + HURT_BAND + 0.05).contains(&d),
                "{d}"
            );
            spread = spread.max(g.distance(at));
        }
        assert!(spread > 5.0, "the guesses vary: {spread}");
        // Point blank, it is still not the exact spot.
        assert!(
            guess(from, from + Vec3::X * 0.5, &mut rng).distance(from + Vec3::X * 0.5)
                >= HURT_MISS - 1e-4
        );
    }

    #[test]
    fn allies_hearing_one_warning_act_on_different_ticks() {
        let p = on();
        let k = Knowledge {
            subject: 9,
            at: Vec3::new(10.0, 0.0, 0.0),
            observed: 500,
            expires: 5000,
        };
        let mut dues = Vec::new();
        for bot in 1..=8u64 {
            let mut s = State::default();
            s.hear(&p, k, Alertness::Combat, bot, 500);
            let due = (500..700).find(|t| s.clone().heard(*t).is_some()).unwrap();
            assert!((530..=620).contains(&due), "bot {bot}: {due}");
            let heard = s.heard(due).unwrap();
            assert!(heard.at.distance(k.at) <= HEAR_OFFSET + 1e-4);
            assert_eq!((heard.subject, heard.observed), (9, 500));
            dues.push(due);
        }
        let mut unique = dues.clone();
        unique.sort_unstable();
        unique.dedup();
        assert!(unique.len() >= 6, "{dues:?}");
        // With strength 0: at once, exactly.
        let plain = BotPerception {
            strength: 0.0,
            ..on()
        };
        let mut s = State::default();
        s.hear(&plain, k, Alertness::Relaxed, 3, 500);
        assert_eq!(s.heard(500).unwrap().at, k.at);
        // A newer warning replaces an older pending one, not the reverse,
        // and a warner repeating itself does not put off acting on it.
        let mut s = State::default();
        s.hear(&p, k, Alertness::Combat, 3, 500);
        s.hear(
            &p,
            Knowledge { observed: 400, ..k },
            Alertness::Combat,
            3,
            500,
        );
        assert_eq!(s.clone().heard(10_000).unwrap().observed, 500);
        let due = (500..700).find(|t| s.clone().heard(*t).is_some()).unwrap();
        for repeat in 1..=6 {
            let observed = 500 + repeat * 30;
            s.hear(
                &p,
                Knowledge { observed, ..k },
                Alertness::Combat,
                3,
                observed,
            );
        }
        assert_eq!(s.heard(due).unwrap().observed, 680);
    }

    #[test]
    fn a_hit_from_out_of_sight_holds_the_look_until_it_reacts() {
        let mut s = State::default();
        let p = on();
        let mut rng = 3;
        let ready = s.react(&p, 0.5, 4, 100, Alertness::Ordinary, true, &mut rng);
        s.reaction.as_mut().unwrap().unseen = true;
        assert!(s.startled(100) && s.startled(ready - 1));
        assert!(!s.startled(ready));
    }

    /// Ticks to turn `degrees`, until within a degree of the aim, and the
    /// most it ever went past; and each tick's turn.
    fn turning(p: &BotPerception, degrees: f32, plain: f32) -> (u64, f32, Vec<f32>) {
        let mut s = State::default();
        let aim = degrees.to_radians();
        let mut yaw = 0.0;
        let mut reached = None;
        let mut past = 0.0f32;
        let mut steps = Vec::new();
        for tick in 0..480u64 {
            let next = s.turn(p, yaw, aim, plain / 120.0);
            steps.push(wrap(next - yaw));
            yaw = next;
            if reached.is_none() && wrap(aim - yaw).abs() < 1f32.to_radians() {
                reached = Some(tick + 1);
            }
            past = past.max(-wrap(aim - yaw) * aim.signum());
        }
        (reached.unwrap(), past.to_degrees(), steps)
    }

    #[test]
    fn a_half_turn_takes_the_plain_time_but_not_at_one_rate() {
        for plain in [90f32.to_radians(), 360f32.to_radians(), 720f32.to_radians()] {
            let linear = (std::f32::consts::PI / (plain / 120.0)).ceil();
            let (reached, past, steps) = turning(&on(), 179.0, plain);
            let ratio = reached as f32 / linear;
            assert!((0.8..=1.2).contains(&ratio), "{reached} vs {linear} ticks");
            // Faster in the middle than at the start or end.
            let moving: Vec<f32> = steps
                .iter()
                .map(|s| s.abs())
                .filter(|s| *s > 1e-6)
                .collect();
            let top = moving.iter().copied().fold(0.0, f32::max);
            assert!(
                moving[0] < top * 0.2 && top > plain / 120.0 * 1.1,
                "{moving:?}"
            );
            // A flick goes a little past, then settles within a degree in
            // the settle time and stays.
            assert!(past > 0.3 && past < 8.0, "{past} degrees past");
            let settled = reached + ticks(SETTLE_SECONDS);
            let mut s = State::default();
            let mut yaw = 0.0;
            for tick in 1..=600 {
                yaw = s.turn(&on(), yaw, 179f32.to_radians(), plain / 120.0);
                if tick >= settled {
                    assert!(
                        wrap(179f32.to_radians() - yaw).abs() < 1f32.to_radians(),
                        "{tick}"
                    );
                }
            }
        }
        // Alertness 0: the plain linear turn.
        let plain = BotPerception {
            strength: 0.0,
            ..on()
        };
        let (reached, past, steps) = turning(&plain, 179.0, 360f32.to_radians());
        assert_eq!(reached, (179.0 / 3.0f32).ceil() as u64);
        assert_eq!(past, 0.0);
        assert!(
            steps[..50]
                .iter()
                .all(|s| (s - 3f32.to_radians()).abs() < 1e-5)
        );
    }

    #[test]
    fn small_corrections_track_without_wobbling() {
        // A target sliding sideways at a radian a second is tracked to
        // well within the fire gate's 0.1 radians.
        let mut s = State::default();
        let mut yaw = 0.0;
        for tick in 0..240 {
            let aim = tick as f32 / 120.0;
            yaw = s.turn(&on(), yaw, aim, 360f32.to_radians() / 120.0);
            if tick > 30 {
                assert!(wrap(aim - yaw).abs() < 0.05, "{tick}: {}", wrap(aim - yaw));
            }
        }
        let (_, past, _) = turning(&on(), 4.0, 360f32.to_radians());
        assert!(past < 1.0, "a small turn barely overshoots: {past}");
    }

    #[test]
    fn an_idle_look_drifts_slowly_and_differs_by_bot() {
        let p = on();
        let a: Vec<f32> = (0..1200).map(|t| drift(&p, 1, t)).collect();
        let most = a.iter().copied().fold(0.0, |m: f32, d| m.max(d.abs()));
        assert!(most <= DRIFT_DEGREES.to_radians() + 1e-5 && most > 2f32.to_radians());
        assert!(
            a.windows(2)
                .all(|w| (w[1] - w[0]).abs() < 0.2f32.to_radians())
        );
        assert_ne!(drift(&p, 1, 500), drift(&p, 2, 500));
        let plain = BotPerception {
            strength: 0.0,
            ..on()
        };
        assert_eq!(drift(&plain, 1, 500), 0.0);
    }

    #[test]
    fn the_fire_gate_judges_the_aim_the_bot_believes() {
        let ideal = Vec3::new(0.3, 0.1, -1.0).normalize();
        let error = (0.08f32, -0.03f32);
        let yaw = ideal.x.atan2(-ideal.z) + error.0;
        let pitch = ideal.y.asin() + error.1;
        let actual = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        assert!(believed(&on(), actual, error).distance(ideal) < 1e-4);
        let plain = BotPerception {
            strength: 0.0,
            ..on()
        };
        assert_eq!(believed(&plain, actual, error), actual);
        // Tracking lags a moving target by the same distance at any range,
        // and keeps up with a still one.
        let near = steady_error(&on(), 5.0, 10.0) * 10.0;
        let far = steady_error(&on(), 5.0, 25.0) * 25.0;
        assert!(near > 0.5 && (near - far).abs() < 1e-4, "{near} {far}");
        assert_eq!(steady_error(&on(), 0.0, 10.0), 0.0);
        assert_eq!(steady_error(&plain, 5.0, 10.0), 0.0);
        assert!(
            steady_error(
                &BotPerception {
                    strength: 2.0,
                    ..on()
                },
                5.0,
                10.0
            ) > steady_error(&on(), 5.0, 10.0)
        );
    }
}
