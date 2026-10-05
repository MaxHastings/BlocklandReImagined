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
    /// A useful, reservable opportunity in the environment.
    Interact,
    /// An enemy in sight within its weapon's band: stand its ground,
    /// strafe and shoot.
    Fight,
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
    /// Utility of an available interaction; zero when there is none.
    pub interaction: f32,
    /// A grounded objective is available.
    pub objective: bool,
    /// The enemy in sight within its chase radius: how far across, and how
    /// much higher.
    pub enemy: Option<(f32, f32)>,
    /// The far edge of its weapon's band.
    pub far: f32,
    /// How high it steps.
    pub step: f32,
    /// How much farther above or below a step its attack reaches from where
    /// it stands: a ranged weapon's band, nothing for a body or melee hit.
    pub reach_up: f32,
    /// It remembers where an enemy was.
    pub remembers: bool,
    /// Farther from its brick than it strolls (never for a rules bot).
    pub strayed: bool,
    /// Its walk back home is done.
    pub home: bool,
}

/// Leeway past its band's far edge before a fighting bot gives chase.
pub(crate) const BAND_SLACK: f32 = 1.0;
/// Leeway in height before a fighting bot gives chase, beyond a step.
const RISE_SLACK: f32 = 1.0;

impl Behaviour {
    /// Every behaviour, most urgent first.
    pub(crate) const ALL: [Behaviour; 8] = [
        Behaviour::Carry,
        Behaviour::Interact,
        Behaviour::Fight,
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

    /// How much it wants the bot now, after `current`: 0 not at all. The
    /// base scores keep the urgency order; weights may reorder them.
    fn score(self, current: Behaviour, s: &Situation) -> f32 {
        let fits = |on: bool, score: f32| if on { score } else { 0.0 };
        match self {
            Behaviour::Carry => fits(s.holding, 1.0),
            Behaviour::Interact => s.interaction,
            Behaviour::Fight => fits(
                s.enemy.is_some_and(|(distance, rise)| {
                    let slack = if current == Behaviour::Fight {
                        BAND_SLACK
                    } else {
                        0.0
                    };
                    distance <= s.far + slack && rise.abs() <= s.step + RISE_SLACK + s.reach_up
                }),
                0.8,
            ),
            Behaviour::Chase => fits(s.enemy.is_some(), 0.6),
            // One lost from sight: one in sight is fought or chased.
            Behaviour::Search => fits(s.remembers && s.enemy.is_none(), 0.4),
            Behaviour::Return => fits(s.strayed || (current == Behaviour::Return && !s.home), 0.3),
            Behaviour::Objective => fits(s.objective, 0.65),
            Behaviour::Wander => 0.1,
        }
    }
}

/// The behaviour to follow now, after `current`, its scores scaled by
/// `weight` (a kind's `behaviours`). Wander when nothing scores.
pub(crate) fn choose(
    current: Behaviour,
    s: &Situation,
    weight: impl Fn(Behaviour) -> f32,
) -> Behaviour {
    let mut best = (Behaviour::Wander, 0.0);
    for b in Behaviour::ALL {
        let score = b.score(current, s) * weight(b);
        if score > best.1 {
            best = (b, score);
        }
    }
    best.0
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
            remembers: true,
            strayed: true,
            ..enemy(3.0)
        };
        assert_eq!(pick(Wander, &all), Carry);
        let seen = Situation {
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
        assert_eq!(
            pick(
                Return,
                &Situation {
                    home: true,
                    ..Default::default()
                }
            ),
            Wander
        );
    }

    #[test]
    fn an_enemy_far_above_is_chased_not_fought() {
        let above = Situation {
            enemy: Some((2.0, 4.0)),
            ..enemy(2.0)
        };
        assert_eq!(pick(Fight, &above), Chase);
        // A ranged weapon whose band reaches up there shoots from here.
        let ranged = Situation {
            reach_up: 6.0,
            ..above
        };
        assert_eq!(pick(Chase, &ranged), Fight);
    }

    /// At the band's edge it keeps doing what it does: no flip-flop as the
    /// enemy steps back and forth across it.
    #[test]
    fn the_band_edge_does_not_flip_flop() {
        assert_eq!(pick(Chase, &enemy(6.5)), Chase);
        assert_eq!(pick(Fight, &enemy(6.5)), Fight);
        assert_eq!(pick(Fight, &enemy(7.5)), Chase);
        assert_eq!(pick(Chase, &enemy(5.5)), Fight);
    }

    /// Once walking home it walks all the way, not just back inside the
    /// edge.
    #[test]
    fn a_walk_home_goes_all_the_way() {
        let near = Situation::default();
        assert_eq!(pick(Return, &near), Return);
        assert_eq!(pick(Return, &Situation { home: true, ..near }), Wander);
        assert_eq!(pick(Wander, &near), Wander);
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
            strayed: true,
            ..far
        };
        assert_eq!(pick(Wander, &lost), Search);
        let homebody = |b: Behaviour| if b == Return { 2.0 } else { 1.0 };
        assert_eq!(choose(Wander, &lost, homebody), Return);
        for (i, b) in Behaviour::ALL.into_iter().enumerate() {
            assert_eq!(b as usize, i, "named in order");
        }
        // With nothing scoring, it wanders.
        assert_eq!(choose(Wander, &far, |_| 0.0), Wander);
    }
}
