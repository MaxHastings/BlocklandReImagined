//! What a bot notices: brief glances, and a reaction time that depends on
//! what it was doing (`docs/architecture/bots.md`, "Noticing").
//!
//! A glance turns the bot's ordinary aim toward something salient for a
//! moment: a blast, a loud sound, someone who keeps looking straight at it,
//! something moving fast. Salience comes from engine data only (a blast's
//! radius and damage, a sound's volume, a look's angle, a speed), never
//! from content names. Glances are rare (a chance by salience), short and
//! cooled down, and only an idle bot takes one: never with an enemy in
//! sight, an objective at hand, something held or a seat taken.
//!
//! A reaction is the delay between perceiving a new target or threat and
//! acting on it, with extra aim wobble that settles. Both scale with how
//! alert the bot was: shorter when already fighting, longer when strolling
//! or playing about, longer again when it faced away. Damage still
//! interrupts at once (the chooser sees it); only the return fire waits.
//!
//! Every number is the kind's `perception` (`bots.json`); a weight of 0
//! turns its part off, and the RNG is the bot's own seeded one.
use super::behaviour::Behaviour;
use super::*;
use crate::bot_kind::BotPerception;

/// Ticks between looks round for watchers and fast bodies.
const POLL_TICKS: u64 = 12;
/// Most stimuli one tick keeps for bots to notice.
const MAX_STIMULI: usize = 32;

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
    Blast { radius: f32, damage: f32 },
    Sound { volume: f32 },
    Gaze,
    Motion { speed: f32 },
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
    pub(in crate::session) fn blast(at: Vec3, radius: f32, damage: f32) -> Self {
        Self {
            at,
            source: Source::Blast { radius, damage },
        }
    }
    pub(in crate::session) fn sound(at: Vec3, volume: f32) -> Self {
        Self {
            at,
            source: Source::Sound { volume },
        }
    }
    /// How much it draws the eye of a bot at `from`: 0 not at all, 1 or
    /// more a sure glance. It falls off to 0 at its reach.
    fn salience(&self, p: &BotPerception, from: Vec3) -> f32 {
        let (weight, reach) = match self.source {
            Source::Blast { radius, damage } => (
                p.blast,
                radius * p.blast_reach + damage.max(0.0) * p.blast_damage_reach,
            ),
            Source::Sound { volume } => (p.sound, volume.clamp(0.0, 1.0) * p.sound_reach),
            Source::Gaze => (p.gaze, p.gaze_range),
            Source::Motion { speed } => (
                p.motion * (speed / p.motion_speed - 1.0).clamp(0.0, 1.0),
                p.motion_range,
            ),
        };
        let distance = from.distance(self.at);
        if !(weight > 0.0 && reach > 0.0 && distance < reach) {
            return 0.0;
        }
        weight * (1.0 - distance / reach)
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
            (Self::Combat, true) => "reacting: in combat, facing away",
            (Self::Ordinary, false) => "reacting",
            (Self::Ordinary, true) => "reacting: facing away",
            (Self::Relaxed, false) => "reacting: relaxed",
            (Self::Relaxed, true) => "reacting: relaxed, facing away",
        }
    }
}
/// Only an idle bot glances: one strolling about or walking home.
pub(super) fn may_glance(behaviour: Behaviour) -> bool {
    matches!(behaviour, Behaviour::Wander | Behaviour::Return)
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
    since: u64,
    ready: u64,
    /// Extra aim error share at `since`, settling to none.
    wobble: f32,
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
    fn glance(
        &mut self,
        p: &BotPerception,
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
            .map(|s| (s.salience(p, eye), s))
            .filter(|(salience, s)| *salience > 0.0 && s.at.distance(eye) > 0.5)
            .max_by(|a, b| a.0.total_cmp(&b.0))?;
        if draw(rng) >= salience.min(1.0) {
            return None;
        }
        let until = tick + ticks(p.glance_seconds * (0.75 + 0.5 * draw(rng))).max(1);
        self.glance = Some(Glance { at: best.at, until });
        self.next_glance = until + ticks(p.glance_cooldown_seconds);
        self.why = Some(BotNotice {
            why: best.source.name(),
            since: tick,
            until,
        });
        Some(best.at)
    }
    /// Someone at `eye` looks straight at it now (or no one): once they
    /// have for the kind's `gaze_seconds`, it is a stimulus, and the stare
    /// counts again from there.
    fn watched(
        &mut self,
        p: &BotPerception,
        tick: u64,
        by: Option<(OwnerId, Vec3)>,
    ) -> Option<Stimulus> {
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
        (tick - since >= ticks(p.gaze_seconds)).then(|| {
            self.watcher = Some((who, tick));
            Stimulus {
                at: eye,
                source: Source::Gaze,
            }
        })
    }
    /// A new target or threat: the tick it may act on it. With the kind's
    /// `reaction` weight 0 this records nothing ([`State::acted`] then
    /// leaves the plain `reaction_seconds` gate in place). A reaction
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
    ) -> Option<u64> {
        let weight = p.reaction;
        if weight <= 0.0 {
            return None;
        }
        if let Some(r) = self
            .reaction
            .filter(|r| r.subject == subject && r.ready > tick)
        {
            return Some(r.ready);
        }
        let scale = match alertness {
            Alertness::Combat => p.combat_scale,
            Alertness::Ordinary => 1.0,
            Alertness::Relaxed => p.relaxed_scale,
        } * if away { p.away_scale } else { 1.0 };
        let scale = (1.0 + weight * (scale - 1.0)).max(0.0);
        let jitter = 1.0 + weight * p.jitter * (draw(rng) * 2.0 - 1.0);
        let ready = tick + ticks(reaction_seconds * scale * jitter);
        self.reaction = Some(Reaction {
            subject,
            since: tick,
            ready,
            wobble: weight * p.wobble * scale,
        });
        self.why = Some(BotNotice {
            why: alertness.name(away),
            since: tick,
            until: ready,
        });
        Some(ready)
    }
    /// Whether its reaction to `subject` has passed; `None` when it has
    /// none for them.
    pub(super) fn acted(&self, subject: OwnerId, tick: u64) -> Option<bool> {
        self.reaction
            .filter(|r| r.subject == subject)
            .map(|r| tick >= r.ready)
    }
    /// What its aim error is multiplied by while it settles on `subject`.
    pub(super) fn wobble(&self, p: &BotPerception, subject: OwnerId, tick: u64) -> f32 {
        self.reaction
            .filter(|r| r.subject == subject)
            .map_or(1.0, |r| {
                let settled = (tick - r.since) as f32 / ticks(p.settle_seconds).max(1) as f32;
                1.0 + r.wobble * (1.0 - settled).max(0.0)
            })
    }
}

impl Brain {
    /// It perceives a new target or threat at `at`, standing at `feet`:
    /// start its reaction (`State::react`) from how alert it was and
    /// whether it faced away. A started reaction draws its aim error afresh,
    /// with the wobble.
    pub(super) fn react_to(&mut self, subject: OwnerId, at: Vec3, feet: Vec3, tick: u64) {
        let to = flat(at - feet);
        let away =
            to.length() > 0.01 && wrap(yaw_to(to) - self.yaw).abs() > std::f32::consts::FRAC_PI_2;
        let alertness = Alertness::of(self.behaviour);
        let Self {
            perception,
            kind,
            rng,
            ..
        } = self;
        let started = perception
            .react(
                &kind.perception,
                kind.reaction_seconds,
                subject,
                tick,
                alertness,
                away,
                rng,
            )
            .is_some()
            && perception.reaction.is_some_and(|r| r.since == tick);
        if started {
            self.next_error = tick;
        }
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
        let p = self.bots.brains.get(&bot)?.kind.perception.clone();
        let mut stimuli = self.bots.stimuli.clone();
        let poll = eligible && (tick + bot).is_multiple_of(POLL_TICKS);
        let watcher = (poll && p.gaze > 0.0)
            .then(|| self.bot_watcher(bot, eye, &p))
            .flatten();
        if poll && p.motion > 0.0 {
            stimuli.extend(
                self.bots
                    .objects
                    .iter()
                    .filter(|v| !v.destroyed)
                    .map(|v| (Vec3::from(v.transform.position), Vec3::from(v.velocity)))
                    .filter(|(at, velocity)| {
                        velocity.length() > p.motion_speed
                            && at.distance(eye) < p.motion_range
                            && self.bot_sees_point(eye, *at)
                    })
                    .map(|(at, velocity)| Stimulus {
                        at,
                        source: Source::Motion {
                            speed: velocity.length(),
                        },
                    }),
            );
        }
        let brain = self.bots.brains.get_mut(&bot)?;
        if !eligible {
            brain.perception.watcher = None;
        } else if poll {
            stimuli.extend(brain.perception.watched(&p, tick, watcher));
        }
        let Brain {
            perception, rng, ..
        } = brain;
        perception.glance(&p, tick, eye, eligible, stimuli, rng)
    }
    /// The nearest player in plain view, within the gaze range, whose look
    /// points within the gaze cone of `bot`'s eye.
    fn bot_watcher(&self, bot: OwnerId, eye: Vec3, p: &BotPerception) -> Option<(OwnerId, Vec3)> {
        let cone = p.gaze_degrees.to_radians().cos();
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
                (distance > 0.5 && distance < p.gaze_range && look.dot(to / distance) >= cone)
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
        BotPerception {
            // Twice the weight: a near event is a sure glance.
            blast: 2.0,
            sound: 2.0,
            gaze: 2.0,
            motion: 2.0,
            reaction: 1.0,
            ..Default::default()
        }
    }
    fn blast_at(x: f32) -> Stimulus {
        Stimulus::blast(Vec3::new(x, 1.0, 0.0), 4.0, 50.0)
    }
    const EYE: Vec3 = Vec3::new(0.0, 1.0, 0.0);

    #[test]
    fn a_salient_blast_in_range_draws_a_glance_only_from_an_idle_bot() {
        let p = on();
        let mut rng = 7;
        let mut idle = State::default();
        assert_eq!(
            idle.glance(&p, 100, EYE, true, [blast_at(2.0)], &mut rng),
            Some(blast_at(2.0).at)
        );
        assert_eq!(idle.why.unwrap().why, "glance: blast");
        // It holds for the glance, then lets go.
        let until = idle.why.unwrap().until;
        assert!(until > 100 && until <= 100 + ticks(p.glance_seconds * 1.25) + 1);
        assert!(
            idle.glance(&p, until - 1, EYE, true, [], &mut rng)
                .is_some()
        );
        assert!(idle.glance(&p, until, EYE, true, [], &mut rng).is_none());
        // Carrying an objective, in combat aim or driving, the caller says
        // not eligible: no glance, and an ongoing one ends.
        let mut busy = State::default();
        assert!(
            busy.glance(&p, 100, EYE, false, [blast_at(2.0)], &mut rng)
                .is_none()
        );
        assert!(busy.why.is_none());
        let mut interrupted = State::default();
        interrupted.glance(&p, 100, EYE, true, [blast_at(2.0)], &mut rng);
        assert!(
            interrupted
                .glance(&p, 101, EYE, false, [], &mut rng)
                .is_none()
        );
        assert!(
            interrupted
                .glance(&p, 102, EYE, true, [], &mut rng)
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
    fn out_of_reach_or_quiet_events_draw_nothing() {
        let p = on();
        let mut rng = 7;
        let mut s = State::default();
        // Radius 4 and damage 50 reach 4 * 8 + 50 * 0.2 = 42 units.
        assert!(
            s.glance(&p, 1, EYE, true, [blast_at(43.0)], &mut rng)
                .is_none()
        );
        let silent = Stimulus::sound(Vec3::new(1.0, 1.0, 0.0), 0.0);
        assert!(s.glance(&p, 2, EYE, true, [silent], &mut rng).is_none());
        let loud = Stimulus::sound(Vec3::new(1.0, 1.0, 0.0), 1.0);
        assert!(s.glance(&p, 3, EYE, true, [loud], &mut rng).is_some());
        assert_eq!(s.why.unwrap().why, "glance: sound");
    }

    #[test]
    fn glances_keep_their_cooldown() {
        let p = on();
        let mut rng = 11;
        let mut s = State::default();
        s.glance(&p, 0, EYE, true, [blast_at(1.0)], &mut rng)
            .unwrap();
        let until = s.why.unwrap().until;
        let cooled = until + ticks(p.glance_cooldown_seconds);
        for tick in until..cooled {
            assert!(
                s.glance(&p, tick, EYE, true, [blast_at(1.0)], &mut rng)
                    .is_none(),
                "{tick}"
            );
        }
        assert!(
            s.glance(&p, cooled, EYE, true, [blast_at(1.0)], &mut rng)
                .is_some()
        );
    }

    #[test]
    fn a_long_stare_is_noticed_and_counts_again_after() {
        let p = on();
        let watcher = Some((9, Vec3::new(5.0, 1.0, 0.0)));
        let mut s = State::default();
        let stare = ticks(p.gaze_seconds);
        assert!(s.watched(&p, 0, watcher).is_none());
        assert!(s.watched(&p, stare - 1, watcher).is_none());
        let seen = s.watched(&p, stare, watcher).unwrap();
        assert_eq!(seen.source, Source::Gaze);
        assert!(s.watched(&p, stare + 1, watcher).is_none());
        // Looking away resets the stare; someone else starts their own.
        assert!(s.watched(&p, stare + 2, None).is_none());
        assert!(s.watched(&p, 2 * stare + 1, watcher).is_none());
        assert!(s.watched(&p, 2 * stare + 1, Some((8, EYE))).is_none());
        let mut rng = 3;
        assert!(s.glance(&p, 0, EYE, true, [seen], &mut rng).is_some());
        assert_eq!(s.why.unwrap().why, "glance: watched");
    }

    #[test]
    fn fast_motion_needs_speed_over_its_threshold() {
        let p = on();
        let at = Vec3::new(3.0, 1.0, 0.0);
        let slow = Stimulus {
            at,
            source: Source::Motion {
                speed: p.motion_speed,
            },
        };
        let fast = Stimulus {
            at,
            source: Source::Motion {
                speed: p.motion_speed * 3.0,
            },
        };
        assert_eq!(slow.salience(&p, EYE), 0.0);
        assert!(fast.salience(&p, EYE) > 0.8);
    }

    #[test]
    fn a_goofing_bot_reacts_slower_and_wobbles_more_than_one_in_combat() {
        let p = on();
        for seed in 1..50u64 {
            let (mut relaxed, mut fighting) = (State::default(), State::default());
            let (mut a, mut b) = (seed, seed);
            let slow = relaxed
                .react(&p, 0.35, 5, 1000, Alertness::Relaxed, false, &mut a)
                .unwrap();
            let quick = fighting
                .react(&p, 0.35, 5, 1000, Alertness::Combat, false, &mut b)
                .unwrap();
            assert!(slow > quick, "seed {seed}: {slow} vs {quick}");
            assert!(relaxed.wobble(&p, 5, 1000) > fighting.wobble(&p, 5, 1000));
            // Facing away, slower again.
            let mut c = seed;
            let away = State::default()
                .react(&p, 0.35, 5, 1000, Alertness::Relaxed, true, &mut c)
                .unwrap();
            assert!(away > slow, "seed {seed}");
        }
        // The wobble settles, and only on that subject.
        let mut s = State::default();
        let mut rng = 1;
        s.react(&p, 0.35, 5, 0, Alertness::Relaxed, false, &mut rng);
        assert!(s.wobble(&p, 5, 0) > 2.0);
        assert_eq!(s.wobble(&p, 5, ticks(p.settle_seconds)), 1.0);
        assert_eq!(s.wobble(&p, 6, 0), 1.0);
        assert_eq!(s.why.unwrap().why, "reacting: relaxed");
    }

    #[test]
    fn delays_vary_by_seed() {
        let p = on();
        let ready: std::collections::BTreeSet<u64> = (1..20u64)
            .map(|mut seed| {
                State::default()
                    .react(&p, 0.35, 5, 0, Alertness::Ordinary, false, &mut seed)
                    .unwrap()
            })
            .collect();
        assert!(ready.len() > 3, "{ready:?}");
    }

    #[test]
    fn hurt_then_seen_keeps_the_pending_reaction() {
        let p = on();
        let mut rng = 2;
        let mut s = State::default();
        let ready = s
            .react(&p, 0.35, 5, 100, Alertness::Relaxed, true, &mut rng)
            .unwrap();
        assert_eq!(s.acted(5, ready - 1), Some(false));
        // Seen a tick later: the same pending reaction, not a new one.
        assert_eq!(
            s.react(&p, 0.35, 5, 101, Alertness::Combat, false, &mut rng),
            Some(ready)
        );
        assert_eq!(s.acted(5, ready), Some(true));
        assert_eq!(s.acted(6, ready), None);
    }

    #[test]
    fn weights_at_zero_turn_noticing_off() {
        let p = BotPerception::default();
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
                source: Source::Motion { speed: 400.0 },
            },
        ]
        .into_iter()
        .enumerate()
        {
            assert!(
                s.glance(&p, tick as u64, EYE, true, [stimulus], &mut rng)
                    .is_none()
            );
        }
        let before = rng;
        assert_eq!(
            s.react(&p, 0.35, 5, 0, Alertness::Relaxed, true, &mut rng),
            None
        );
        // Nothing drawn, nothing recorded: the plain reaction gate applies.
        assert_eq!(rng, before);
        assert_eq!(s.acted(5, 0), None);
        assert_eq!(s.wobble(&p, 5, 0), 1.0);
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
