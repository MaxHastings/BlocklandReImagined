//! Which behaviour a bot follows: one at a time, picked each tick by
//! scores. Each behaviour scores how much it wants the bot from what the
//! bot knows (`Situation`), with leeway at the edges so it does not flip
//! between two; the kind's `behaviours` weights scale the scores (0 turns
//! one off), and the highest wins (`docs/architecture/bots.md`). Each
//! behaviour then decides the bot's goal and how it moves; aiming and the
//! trigger follow the enemy whatever the behaviour, unless it is carrying
//! something.
//!
//! A new behaviour is a variant here with its name and score, and its
//! goal and movement in `step_bot`.

/// What a bot is doing, most urgent first (the order breaks ties).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Behaviour {
    /// Holding something with its tool: carry it out into the open and
    /// fling it.
    Carry,
    /// An enemy up where no walk leads: jet up and over to them.
    Fly,
    /// A useful, reservable opportunity in the environment.
    Interact,
    /// An enemy in sight within its weapon's band: stand its ground,
    /// strafe and shoot.
    Fight,
    /// Nothing to attack with, an enemy about and no peaceful objective:
    /// go and pick up a weapon it sees lying in reach before anything but
    /// carrying (`arming`).
    Arm,
    /// An enemy in sight out of its band: go after them.
    Chase,
    /// An enemy it lost from sight, or one that hurt it: go where they
    /// were and look around.
    Search,
    /// Strayed too far from its brick: walk back.
    Return,
    /// A grounded authored objective, interrupted by immediate combat.
    Objective,
    /// Nothing to do: stroll about.
    #[default]
    Wander,
}

/// What a bot knows this tick that decides its behaviour.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Situation {
    /// Its tool holds something.
    pub holding: bool,
    /// An enemy it can fly to where its path does not walk up
    /// (`Session::air_chase`).
    pub fly: bool,
    /// Utility of an available interaction; zero when there is none.
    pub interaction: f32,
    /// A grounded objective is available.
    pub objective: bool,
    /// It carries what that objective delivers: it keeps going (shooting
    /// back on the way) rather than stopping to fight or fly.
    pub committed: bool,
    /// It can shoot an enemy in sight on the way there (a ranged weapon,
    /// an objective that needs only its feet): it goes on, shooting,
    /// rather than standing to fight.
    pub gunning: bool,
    /// How much it wants an item it sees lying in reach: [`arming::ARM`]
    /// with no attack, else an upgrade's score (`arming::upgrade_score`);
    /// 0 for none.
    pub arm: f32,
    /// The enemy in sight within its chase radius: how far across, and how
    /// much higher.
    pub enemy: Option<(f32, f32)>,
    /// The far edge of its weapon's band.
    pub far: f32,
    /// How high it steps.
    pub step: f32,
    /// It remembers where an enemy was.
    pub remembers: bool,
    /// How far past the edge of its stroll it is, 0 inside it to 1 a few
    /// units out (always 0 for a rules bot).
    pub strayed: f32,
    /// A step of an objective the game still offers failed and is cooling
    /// down: it does not walk home meanwhile.
    pub pursuing: bool,
}

/// An objective it can run and gun on the way to, against Fight's 0.8.
const GUNNING: f32 = 0.65;
/// Height an enemy may stand above or below it, beyond a step, and still
/// be fought at full score.
const RISE_SLACK: f32 = 1.0;
/// Over how many units past its band's far edge (or past that height) a
/// fight's score fades to nothing, at least: a fight just past the edge
/// still scores, so the hold rule's margin, not a threshold, decides when
/// it turns into a chase.
const FIGHT_FADE: f32 = 4.5;
/// The share of a band's far edge it fades over, when that is more.
const FIGHT_FADE_SHARE: f32 = 0.5;
/// Behaviours that take over at once when they win: a catch in its tool,
/// and arming an empty hand.
pub(crate) const MUST: [u32; 2] = [Behaviour::Carry as u32, Behaviour::Arm as u32];
/// The same for an armed bot: an upgrade is weighed by the hold rule.
pub(crate) const MUST_ARMED: [u32; 1] = [Behaviour::Carry as u32];

/// 1 up to `edge`, falling to 0 over `width` past it.
fn fade(value: f32, edge: f32, width: f32) -> f32 {
    (1.0 - (value - edge).max(0.0) / width.max(1e-3)).clamp(0.0, 1.0)
}

impl Behaviour {
    /// Every behaviour, most urgent first.
    pub(crate) const ALL: [Behaviour; 10] = [
        Behaviour::Carry,
        Behaviour::Fly,
        Behaviour::Interact,
        Behaviour::Fight,
        Behaviour::Arm,
        Behaviour::Chase,
        Behaviour::Search,
        Behaviour::Return,
        Behaviour::Objective,
        Behaviour::Wander,
    ];

    /// Its name in a kind's `behaviours` weights.
    pub(crate) fn name(self) -> &'static str {
        crate::bot_kind::BEHAVIOURS[self as usize]
    }

    /// How much it wants the bot now: 0 not at all. The base scores keep
    /// the urgency order; weights may reorder them. A score says only what
    /// is true now: holding a choice is the hold rule's job ([`Hold`]).
    fn score(self, s: &Situation) -> f32 {
        let fits = |on: bool, score: f32| if on { score } else { 0.0 };
        match self {
            Behaviour::Carry => fits(s.holding, 1.0),
            Behaviour::Fly => fits(s.fly, 0.9),
            Behaviour::Interact => s.interaction,
            // Full in its band, fading past the far edge and with height.
            Behaviour::Fight => s.enemy.map_or(0.0, |(distance, rise)| {
                let width = (s.far * FIGHT_FADE_SHARE).max(FIGHT_FADE);
                0.8 * fade(distance, s.far, width)
                    * fade(rise.abs(), s.step + RISE_SLACK, FIGHT_FADE)
            }),
            // Empty-handed, before going after an enemy or an objective
            // that wants one beaten; armed, an upgrade by its worth.
            Behaviour::Arm => s.arm,
            Behaviour::Chase => fits(s.enemy.is_some(), 0.6),
            // One lost from sight: one in sight is fought or chased.
            Behaviour::Search => fits(s.remembers && s.enemy.is_none(), 0.4),
            Behaviour::Return => fits(!s.pursuing, 0.3 * s.strayed.clamp(0.0, 1.0)),
            Behaviour::Objective => fits(
                s.objective,
                if s.committed {
                    0.92
                } else if s.gunning {
                    GUNNING
                } else {
                    0.65
                },
            ),
            Behaviour::Wander => 0.1,
        }
    }
}

/// The behaviour the scores alone pick (the hold rule aside), its scores
/// scaled by `weight` (a kind's `behaviours`). Wander when nothing scores.
#[cfg(test)]
pub(crate) fn plain(s: &Situation, weight: impl Fn(Behaviour) -> f32) -> Behaviour {
    best(&scores(s, weight))
}

/// What follows `current` (held since long ago) by the hold rule, with no
/// interrupt: for tests of the scores and the rule together.
#[cfg(test)]
pub(crate) fn choose(
    current: Behaviour,
    s: &Situation,
    weight: impl Fn(Behaviour) -> f32,
) -> Behaviour {
    let scores = scores(s, weight);
    let options: Vec<(u32, f32)> = Behaviour::ALL
        .iter()
        .map(|b| (*b as u32, scores[*b as usize]))
        .collect();
    let mut hold = Hold {
        option: Some(current as u32),
        since: 0,
        scored: 0,
    };
    let ask = Ask {
        options: &options,
        interrupt: false,
        paused: false,
        must: &MUST,
    };
    Behaviour::ALL[hold.choose(BotHold::default(), &ask, 1_000_000).0 as usize]
}

/// Every behaviour's weighted score, in [`Behaviour::ALL`] order.
pub(crate) fn scores(s: &Situation, weight: impl Fn(Behaviour) -> f32) -> [f32; 10] {
    Behaviour::ALL.map(|b| b.score(s) * weight(b))
}

/// The highest of `scores`, the earlier on a tie; Wander when none scores.
#[cfg(test)]
pub(crate) fn best(scores: &[f32; 10]) -> Behaviour {
    let mut best = (Behaviour::Wander, 0.0);
    for b in Behaviour::ALL {
        let score = scores[b as usize];
        if score > best.1 {
            best = (b, score);
        }
    }
    best.0
}

use crate::bot_kind::BotHold;

/// The one rule for holding a choice, whatever is chosen (a behaviour, a
/// weapon, an aim, a route, a goof): what is chosen is held at least
/// [`BotHold::seconds`]; after that another takes over only by scoring
/// more than [`BotHold::margin`] above it. An interrupt takes over at
/// once: one the caller names (urgent damage, an objective picked up or
/// dropped, the target lost or dead), a [`Ask::must`] option winning, or
/// the held option no longer possible (scoring nothing). A held option
/// that pauses between steps ([`Ask::paused`]) is held through the pause
/// for as long as a choice is held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Hold {
    /// What is chosen.
    pub option: Option<u32>,
    /// Since when.
    pub since: u64,
    /// When it last scored.
    pub scored: u64,
}
/// One choice to make.
pub(crate) struct Ask<'a> {
    /// Each option and its score; 0 (or not finite) is not possible now.
    pub options: &'a [(u32, f32)],
    /// Something happened that the choice must answer at once.
    pub interrupt: bool,
    /// The held option scores nothing only because it is between steps.
    pub paused: bool,
    /// Options that take over at once when they win.
    pub must: &'a [u32],
}
/// Why the rule kept or changed a choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Held {
    /// The first choice.
    First,
    /// The best is what is held.
    Best,
    /// Held: chosen too recently to change.
    Committed,
    /// Held: the best does not beat it by the margin.
    Margin,
    /// Held: between steps.
    Paused,
    /// Changed: the best beat it by the margin.
    Beaten,
    /// Changed at once: an interrupt, or a must-do option.
    Interrupt,
    /// Changed at once: the held option is no longer possible.
    Impossible,
}
impl Held {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Best => "best",
            Self::Committed => "committed",
            Self::Margin => "margin",
            Self::Paused => "paused",
            Self::Beaten => "beaten",
            Self::Interrupt => "interrupt",
            Self::Impossible => "impossible",
        }
    }
}
impl Hold {
    /// Choose among `ask`'s options at `tick`, holding the last choice by
    /// `rule`.
    pub(crate) fn choose(&mut self, rule: BotHold, ask: &Ask, tick: u64) -> (u32, Held) {
        let score = |o: u32| {
            ask.options
                .iter()
                .find(|(x, _)| *x == o)
                .map_or(0.0, |(_, s)| if s.is_finite() { s.max(0.0) } else { 0.0 })
        };
        // The best now, the earlier on a tie.
        let mut best: Option<(u32, f32)> = None;
        for (option, _) in ask.options {
            let s = score(*option);
            if s > 0.0 && best.is_none_or(|(_, b)| s > b) {
                best = Some((*option, s));
            }
        }
        let commit = (rule.seconds.max(0.0) * 120.0).round() as u64;
        let Some(current) = self.option else {
            let first = best.map_or(ask.options.first().map_or(0, |o| o.0), |b| b.0);
            *self = Hold {
                option: Some(first),
                since: tick,
                scored: tick,
            };
            return (first, Held::First);
        };
        let held = score(current);
        if held > 0.0 {
            self.scored = tick;
        }
        let Some((challenger, challenge)) = best.filter(|(b, _)| *b != current) else {
            return (current, Held::Best);
        };
        let why = if ask.interrupt || ask.must.contains(&challenger) {
            Held::Interrupt
        } else if held <= 0.0 {
            if ask.paused && tick < self.scored + commit {
                return (current, Held::Paused);
            }
            Held::Impossible
        } else if tick < self.since + commit {
            return (current, Held::Committed);
        } else if challenge > held * (1.0 + rule.margin) {
            Held::Beaten
        } else {
            return (current, Held::Margin);
        };
        *self = Hold {
            option: Some(challenger),
            since: tick,
            scored: tick,
        };
        (challenger, why)
    }
}

#[cfg(test)]
mod tests {
    use super::Behaviour::*;
    use super::*;

    fn pick(current: Behaviour, s: &Situation) -> Behaviour {
        choose(current, s, |_| 1.0)
    }

    fn enemy(distance: f32) -> Situation {
        Situation {
            enemy: Some((distance, 0.0)),
            far: 6.0,
            step: 0.6,
            ..Default::default()
        }
    }

    #[test]
    fn the_most_urgent_behaviour_wins() {
        let all = Situation {
            holding: true,
            fly: true,
            remembers: true,
            strayed: 1.0,
            ..enemy(3.0)
        };
        assert_eq!(pick(Wander, &all), Carry);
        assert_eq!(
            pick(
                Wander,
                &Situation {
                    holding: false,
                    ..all
                }
            ),
            Fly
        );
        let seen = Situation {
            fly: false,
            holding: false,
            ..all
        };
        assert_eq!(pick(Wander, &seen), Fight);
        assert_eq!(
            pick(
                Wander,
                &Situation {
                    enemy: Some((20.0, 0.0)),
                    ..seen
                }
            ),
            Chase
        );
        let lost = Situation {
            enemy: None,
            ..seen
        };
        assert_eq!(pick(Fight, &lost), Search);
        assert_eq!(
            pick(
                Search,
                &Situation {
                    remembers: false,
                    ..lost
                }
            ),
            Return
        );
        assert_eq!(pick(Return, &Situation::default()), Wander);
    }

    #[test]
    fn an_enemy_far_above_is_chased_not_fought() {
        let above = Situation {
            enemy: Some((2.0, 4.0)),
            ..enemy(2.0)
        };
        assert_eq!(pick(Fight, &above), Chase);
    }

    /// At the band's edge it keeps doing what it does: no flip-flop as the
    /// enemy steps back and forth across it. The fight's score fades past
    /// the edge; the hold rule's margin keeps either choice there.
    #[test]
    fn the_band_edge_does_not_flip_flop() {
        assert_eq!(pick(Chase, &enemy(7.0)), Chase);
        assert_eq!(pick(Fight, &enemy(7.0)), Fight);
        assert_eq!(pick(Fight, &enemy(7.6)), Chase);
        assert_eq!(pick(Chase, &enemy(6.5)), Fight);
        // Nor as it jumps: a little above is still fought once fighting.
        let jumped = Situation {
            enemy: Some((4.0, 2.5)),
            ..enemy(4.0)
        };
        assert_eq!(pick(Fight, &jumped), Fight);
        assert_eq!(pick(Chase, &jumped), Chase);
    }

    /// With nothing to attack with and a weapon in sight, it arms itself
    /// before it goes after an enemy or an objective that wants one beaten;
    /// a carry still comes first.
    #[test]
    fn an_empty_handed_bot_arms_before_its_objective() {
        let unarmed = Situation {
            arm: 0.95,
            objective: true,
            ..Default::default()
        };
        assert_eq!(pick(Wander, &unarmed), Arm);
        assert_eq!(pick(Objective, &unarmed), Arm);
        assert_eq!(
            pick(
                Fight,
                &Situation {
                    arm: 0.95,
                    ..enemy(2.0)
                }
            ),
            Arm
        );
        assert_eq!(
            pick(
                Chase,
                &Situation {
                    arm: 0.95,
                    ..enemy(20.0)
                }
            ),
            Arm
        );
        assert_eq!(
            pick(
                Arm,
                &Situation {
                    holding: true,
                    ..unarmed
                }
            ),
            Carry
        );
        assert_eq!(
            pick(
                Arm,
                &Situation {
                    arm: 0.0,
                    ..unarmed
                }
            ),
            Objective
        );
    }

    /// A long band fades over a longer way: a sniper at 40 keeps fighting
    /// an enemy that steps a few units past it.
    #[test]
    fn a_long_band_holds_its_fight_further_out() {
        let sniper = |distance: f32| Situation {
            far: 40.0,
            ..enemy(distance)
        };
        assert_eq!(pick(Fight, &sniper(45.0)), Fight);
        assert_eq!(pick(Fight, &sniper(47.0)), Chase);
        assert_eq!(pick(Chase, &sniper(44.0)), Chase);
    }

    /// Carrying what its objective delivers, it keeps going past an enemy
    /// in its band, or one up high; only a carry of its own comes first.
    #[test]
    fn a_carrier_delivers_rather_than_stopping_to_fight() {
        let carrying = Situation {
            objective: true,
            committed: true,
            fly: true,
            ..enemy(3.0)
        };
        assert_eq!(pick(Fight, &carrying), Objective);
        assert_eq!(
            pick(
                Objective,
                &Situation {
                    committed: false,
                    ..carrying
                }
            ),
            Fly
        );
        assert_eq!(
            pick(
                Objective,
                &Situation {
                    holding: true,
                    ..carrying
                }
            ),
            Carry
        );
    }

    /// Walking home it does not flicker at the edge of its stroll: the
    /// hold rule's margin keeps either choice a little way either side.
    #[test]
    fn a_walk_home_does_not_flicker_at_the_edge() {
        let edge = Situation {
            strayed: 0.35,
            ..Default::default()
        };
        assert_eq!(pick(Return, &edge), Return);
        assert_eq!(pick(Wander, &edge), Wander);
        let back = Situation {
            strayed: 0.2,
            ..Default::default()
        };
        assert_eq!(pick(Return, &back), Wander);
        let out = Situation {
            strayed: 0.5,
            ..Default::default()
        };
        assert_eq!(pick(Wander, &out), Return);
    }

    /// A bot whose objective step failed and is cooling down (it timed out)
    /// does not walk home meanwhile.
    #[test]
    fn a_bot_after_an_objective_does_not_walk_home() {
        let strayed = Situation {
            strayed: 1.0,
            ..Default::default()
        };
        assert_eq!(pick(Wander, &strayed), Return);
        let pursuing = Situation {
            pursuing: true,
            ..strayed
        };
        assert_eq!(pick(Wander, &pursuing), Wander);
        assert_eq!(pick(Return, &pursuing), Wander);
    }

    /// A kind's weights turn a behaviour off or put it first.
    #[test]
    fn weights_reorder_or_turn_off_behaviours() {
        let far = enemy(20.0);
        assert_eq!(pick(Wander, &far), Chase);
        // A guard that never gives chase stays at its post.
        let guard = |b: Behaviour| if b == Chase { 0.0 } else { 1.0 };
        assert_eq!(choose(Wander, &far, guard), Wander);
        // One that would rather go home than search once it strays.
        let lost = Situation {
            enemy: None,
            remembers: true,
            strayed: 1.0,
            ..far
        };
        assert_eq!(pick(Wander, &lost), Search);
        let homebody = |b: Behaviour| if b == Return { 2.0 } else { 1.0 };
        assert_eq!(choose(Wander, &lost, homebody), Return);
        assert_eq!(plain(&lost, homebody), Return);
        for (i, b) in Behaviour::ALL.into_iter().enumerate() {
            assert_eq!(b as usize, i, "named in order");
        }
        // With nothing scoring, it wanders.
        assert_eq!(choose(Wander, &far, |_| 0.0), Wander);
    }

    const RULE: BotHold = BotHold {
        seconds: 0.5,
        margin: 0.1,
    };
    fn held(option: u32, since: u64) -> Hold {
        Hold {
            option: Some(option),
            since,
            scored: since,
        }
    }
    fn ask(options: &[(u32, f32)]) -> Ask<'_> {
        Ask {
            options,
            interrupt: false,
            paused: false,
            must: &[],
        }
    }

    /// A challenger that does not beat the held choice by the margin never
    /// takes over, however long it waits; one that does, takes over once
    /// the choice has been held long enough.
    #[test]
    fn a_challenger_below_the_margin_does_not_switch() {
        let mut h = held(0, 0);
        for tick in 0..10_000 {
            let (o, why) = h.choose(RULE, &ask(&[(0, 1.0), (1, 1.09)]), tick);
            assert_eq!(o, 0, "{why:?} at {tick}");
        }
        let mut h = held(0, 0);
        let (o, why) = h.choose(RULE, &ask(&[(0, 1.0), (1, 1.2)]), 30);
        assert_eq!((o, why), (0, Held::Committed));
        let (o, why) = h.choose(RULE, &ask(&[(0, 1.0), (1, 1.2)]), 60);
        assert_eq!((o, why), (1, Held::Beaten));
        assert_eq!(h.since, 60);
        // And the new choice is held in turn.
        let (o, _) = h.choose(RULE, &ask(&[(0, 2.0), (1, 1.2)]), 61);
        assert_eq!(o, 1);
    }

    /// Each interrupt kind takes over at once from a choice just made:
    /// one the caller names (urgent damage, an objective picked up or
    /// dropped, a target lost or dead), a must-do option, and the held
    /// option no longer possible.
    #[test]
    fn a_held_choice_yields_at_once_to_each_interrupt() {
        let options = [(0, 1.0), (1, 1.01)];
        let mut h = held(0, 100);
        let named = Ask {
            interrupt: true,
            ..ask(&options)
        };
        assert_eq!(h.choose(RULE, &named, 101), (1, Held::Interrupt));
        let mut h = held(0, 100);
        let must = Ask {
            must: &[1],
            ..ask(&options)
        };
        assert_eq!(h.choose(RULE, &must, 101), (1, Held::Interrupt));
        let mut h = held(0, 100);
        assert_eq!(
            h.choose(RULE, &ask(&[(0, 0.0), (1, 0.1)]), 101),
            (1, Held::Impossible)
        );
        let mut h = held(0, 100);
        assert_eq!(
            h.choose(RULE, &ask(&[(0, f32::NAN), (1, 0.1)]), 101),
            (1, Held::Impossible)
        );
        // Without one, the same choice holds.
        let mut h = held(0, 100);
        assert_eq!(h.choose(RULE, &ask(&options), 101), (0, Held::Committed));
    }

    /// Between an objective's steps it scores nothing for a moment: the
    /// rule holds it through the pause (no blink to Wander), and lets go
    /// once the pause outlasts a held choice.
    #[test]
    fn an_objective_is_held_between_its_steps() {
        let objective = Objective as u32;
        let wander = Wander as u32;
        let mut h = held(objective, 0);
        let during = [(objective, 0.65), (wander, 0.1)];
        let between = [(objective, 0.0), (wander, 0.1)];
        assert_eq!(h.choose(RULE, &ask(&during), 1000).0, objective);
        let paused = |options| Ask {
            paused: true,
            ..ask(options)
        };
        for tick in 1001..1060 {
            assert_eq!(
                h.choose(RULE, &paused(&between), tick),
                (objective, Held::Paused)
            );
        }
        assert_eq!(h.choose(RULE, &ask(&during), 1060).0, objective);
        // A pause longer than a held choice ends it.
        for tick in 1061..1120 {
            assert_eq!(h.choose(RULE, &paused(&between), tick).0, objective);
        }
        assert_eq!(
            h.choose(RULE, &paused(&between), 1120),
            (wander, Held::Impossible)
        );
        // Not paused, it lets go at once.
        let mut h = held(objective, 0);
        assert_eq!(
            h.choose(RULE, &ask(&between), 10),
            (wander, Held::Impossible)
        );
    }
}
