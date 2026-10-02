//! Which behaviour a bot follows: one at a time, picked each tick by a
//! fixed order of urgency from what it knows (`Situation`), with leeway
//! at the edges so it does not flip between two
//! (`docs/architecture/bots.md`). Each behaviour then decides the bot's
//! goal and how it moves; aiming and the trigger follow the enemy
//! whatever the behaviour, unless it is carrying something.
//!
//! A new behaviour is a variant here, a rule in [`choose`] at its place in
//! the order, and its goal and movement in `step_bot`.

/// What a bot is doing, most urgent first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Behaviour {
    /// Holding something with its tool: carry it out into the open and
    /// fling it.
    Carry,
    /// An enemy up where no walk leads: jet up and over to them.
    Fly,
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
    /// The enemy in sight within its chase radius: how far across, and how
    /// much higher.
    pub enemy: Option<(f32, f32)>,
    /// The far edge of its weapon's band.
    pub far: f32,
    /// How high it steps.
    pub step: f32,
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

/// The behaviour to follow now, after `current`.
pub(crate) fn choose(current: Behaviour, s: &Situation) -> Behaviour {
    use Behaviour::*;
    if s.holding {
        return Carry;
    }
    if s.fly {
        return Fly;
    }
    if let Some((distance, rise)) = s.enemy {
        let slack = if current == Fight { BAND_SLACK } else { 0.0 };
        return if distance <= s.far + slack && rise.abs() <= s.step + RISE_SLACK {
            Fight
        } else {
            Chase
        };
    }
    if s.remembers {
        return Search;
    }
    if s.strayed || (current == Return && !s.home) {
        return Return;
    }
    Wander
}

#[cfg(test)]
mod tests {
    use super::Behaviour::*;
    use super::*;

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
            strayed: true,
            ..enemy(3.0)
        };
        assert_eq!(choose(Wander, &all), Carry);
        assert_eq!(
            choose(
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
        assert_eq!(choose(Wander, &seen), Fight);
        assert_eq!(
            choose(
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
        assert_eq!(choose(Fight, &lost), Search);
        assert_eq!(
            choose(
                Search,
                &Situation {
                    remembers: false,
                    ..lost
                }
            ),
            Return
        );
        assert_eq!(
            choose(
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
        assert_eq!(choose(Fight, &above), Chase);
    }

    /// At the band's edge it keeps doing what it does: no flip-flop as the
    /// enemy steps back and forth across it.
    #[test]
    fn the_band_edge_does_not_flip_flop() {
        assert_eq!(choose(Chase, &enemy(6.5)), Chase);
        assert_eq!(choose(Fight, &enemy(6.5)), Fight);
        assert_eq!(choose(Fight, &enemy(7.5)), Chase);
        assert_eq!(choose(Chase, &enemy(5.5)), Fight);
    }

    /// Once walking home it walks all the way, not just back inside the
    /// edge.
    #[test]
    fn a_walk_home_goes_all_the_way() {
        let near = Situation::default();
        assert_eq!(choose(Return, &near), Return);
        assert_eq!(choose(Return, &Situation { home: true, ..near }), Wander);
        assert_eq!(choose(Wander, &near), Wander);
    }
}
