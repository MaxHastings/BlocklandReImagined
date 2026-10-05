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
//! The kind's `perception` (`bots.json`) holds seven numbers: `salience`
//! (scales every source's reach, 0 off), `glance_seconds`,
//! `cooldown_seconds`, `alertness` (0 to 1, scales the delay, aim error,
//! view-cone delay and turn cap together, 0 plain), `relaxed_scale`,
//! `away_scale`, `view_degrees`. Reaches come from engine data (blast
//! radius, sound volume, the kind's sight, the body's running speed) times
//! fixed constants below. The RNG is the bot's own seeded one.
use super::behaviour::Behaviour;
use super::*;
use crate::bot_kind::BotPerception;

/// Ticks between looks round for watchers and fast bodies.
const POLL_TICKS: u64 = 12;
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
/// A body moving faster than this many times the bot's own running speed
/// is moving fast; it is fully salient at twice that.
const FAST: f32 = 2.0;
/// Seconds the starting aim error takes to narrow (`bots.rs`' tracking).
const SETTLE_SECONDS: f32 = 2.0;
/// A reaction delay varies by up to this share either way.
const JITTER: f32 = 0.3;

/// The bots' seeded generator: the next value in [0, 1).
pub(super) fn draw(rng: &mut u64) -> f32 {
    *rng = rng
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*rng >> 40) as f32 / (1u64 << 24) as f32
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
/// Only an idle bot glances: one strolling about or walking home.
pub(super) fn may_glance(behaviour: Behaviour) -> bool {
    matches!(behaviour, Behaviour::Wander | Behaviour::Return)
}
/// How much slower than plain it reacts (and how much wider its first aim
/// errs): by alertness, and more from outside its view cone, blended by
/// the kind's `reaction` weight.
fn scale(p: &BotPerception, alertness: Alertness, away: bool) -> f32 {
    let scale = match alertness {
        // Fighting or hunting: the kind's plain numbers.
        Alertness::Combat => 1.0,
        Alertness::Ordinary => 1.0,
        Alertness::Relaxed => p.relaxed_scale,
    } * if away { p.away_scale } else { 1.0 };
    (1.0 + p.alertness * (scale - 1.0)).max(0.0)
}
/// Ticks of delay before acting on something new: the kind's plain
/// `reaction_seconds` scaled by alertness and view, varied by `JITTER`.
/// With the kind's `alertness` 0, exactly `reaction_seconds`, drawing
/// nothing. Any pause before acting on a change uses this.
pub(super) fn delay_ticks(
    p: &BotPerception,
    reaction_seconds: f32,
    alertness: Alertness,
    away: bool,
    rng: &mut u64,
) -> u64 {
    if p.alertness <= 0.0 {
        return ticks(reaction_seconds);
    }
    let jitter = 1.0 + p.alertness * JITTER * (draw(rng) * 2.0 - 1.0);
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
}
/// A brain's noticing.
#[derive(Clone, Debug, Default)]
pub(super) struct State {
    glance: Option<Glance>,
    next_glance: u64,
    /// Who has been looking straight at it, since when.
    watcher: Option<(OwnerId, u64)>,
    reaction: Option<Reaction>,
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
        });
        if p.alertness > 0.0 {
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
    /// What its turn rate toward `subject` is multiplied by now: slower
    /// toward one from outside its view cone, until it has reacted.
    pub(super) fn turn_scale(&self, subject: OwnerId, tick: u64) -> f32 {
        self.reaction
            .filter(|r| r.subject == subject && tick < r.ready)
            .map_or(1.0, |r| r.turn)
    }
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
        let alertness = Alertness::of(self.behaviour);
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
        if kind.perception.alertness > 0.0 && perception.why.is_some_and(|w| w.since == tick) {
            self.next_error = tick;
        }
    }
    /// It was hurt by `k.subject`, not the target it is fighting: with the
    /// reaction model on, its return fire waits a reaction.
    pub(super) fn hurt_by(&mut self, k: &Knowledge, feet: Vec3, tick: u64) {
        if self.kind.perception.alertness > 0.0 && self.target != Some(k.subject) {
            self.perceive(k.subject, k.at, feet, tick, true, true);
        }
    }
    /// Ticks to pause before acting on a change of mind (a chooser's tell):
    /// the same reaction delay, by how alert it is now.
    #[allow(dead_code)]
    pub(super) fn switch_delay(&mut self) -> u64 {
        let alertness = Alertness::of(self.behaviour);
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
    /// Where `bot` glances this tick (`State::glance`). `eligible` is the
    /// caller's: idle, nothing in sight, no objective, nothing held, not
    /// seated or driving.
    pub(super) fn bot_glance(
        &mut self,
        bot: OwnerId,
        tick: u64,
        eye: Vec3,
        eligible: bool,
    ) -> Option<Vec3> {
        let kind = &self.bots.brains.get(&bot)?.kind;
        let (p, sight) = (kind.perception.clone(), kind.sight);
        let reach = sight * SEEN_REACH * p.salience;
        let fast = FAST * self.peers.get(&bot)?.player.tuning().forward.max(0.1);
        let mut stimuli = self.bots.stimuli.clone();
        let poll = eligible && reach > 0.0 && (tick + bot).is_multiple_of(POLL_TICKS);
        let watcher = poll.then(|| self.bot_watcher(bot, eye, reach)).flatten();
        if poll {
            stimuli.extend(
                self.bots
                    .objects
                    .iter()
                    .filter(|v| !v.destroyed)
                    .map(|v| (Vec3::from(v.transform.position), Vec3::from(v.velocity)))
                    .filter(|(at, velocity)| {
                        velocity.length() > fast
                            && at.distance(eye) < reach
                            && self.bot_sees_point(eye, *at)
                    })
                    .map(|(at, velocity)| Stimulus {
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
    /// The nearest player in plain view, within `range`, whose look points
    /// within `GAZE_DEGREES` of `bot`'s eye.
    fn bot_watcher(&self, bot: OwnerId, eye: Vec3, range: f32) -> Option<(OwnerId, Vec3)> {
        let cone = GAZE_DEGREES.to_radians().cos();
        self.peers
            .iter()
            .filter(|(owner, peer)| **owner != bot && peer.combat.alive)
            .filter_map(|(owner, peer)| {
                let state = peer.player.state();
                let from = peer.player.eye();
                let to = eye - from;
                let distance = to.length();
                let look = Vec3::new(
                    state.yaw.sin() * state.pitch.cos(),
                    state.pitch.sin(),
                    -state.yaw.cos() * state.pitch.cos(),
                );
                (distance > 0.5 && distance < range && look.dot(to / distance) >= cone)
                    .then_some((*owner, from, distance))
            })
            .filter(|(_, from, _)| self.bot_sees_point(eye, *from))
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(owner, from, _)| (owner, from))
    }
    /// Nothing solid between `eye` and `at`.
    fn bot_sees_point(&self, eye: Vec3, at: Vec3) -> bool {
        let to = at - eye;
        let distance = to.length();
        distance < 0.01
            || super::super::admin_players::world_ray(
                &self.simulation,
                eye,
                to / distance,
                distance,
            )
            .is_none_or(|hit| hit >= distance - 0.3)
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
            alertness: 0.0,
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
}
