//! The act stage: the one place a bot's choices become its walk and its
//! buttons this tick.
//!
//! The chooser has picked a behaviour; its route, its stance in a fight, a
//! jet leg, a swim, its team's room, a dodge, a goof and the like each
//! *propose* how to move ([`Proposal`]). They never write the controls.
//! [`resolve`] decides once, in one fixed priority order ([`Mover`]): the
//! highest mover with a walk of its own sets the walk, the buttons a mover
//! sets are set in that order, and presses (a hop, a crouch) are added on
//! top. Safety comes last and only takes away (`Session::bot_safe_walk`):
//! never through a portal off the route, round a vehicle in the way, and
//! never off an edge whose fall would hurt.
//!
//! So a mover added later goes in at its place in [`Mover`], not after
//! whatever happened to run last.
use super::*;

/// Who proposes, lowest priority first: a later mover's walk replaces an
/// earlier one's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Mover {
    /// Along the planned route (or through the opening it leads through).
    Route,
    /// Into the body it works (an interaction's push).
    Push,
    /// Its stance where it fights or works: a weave, melee footwork, a
    /// ranged strafe and lean, giving ground.
    Stance,
    /// A jet leg of its route, flying itself.
    Jet,
    /// Afloat, or a swimmer rising and diving.
    Swim,
    /// Out of where a teammate's weapon will hit.
    Team,
    /// A goof's walk (a circle, a detour, up to someone).
    Goof,
    /// A dodge's step aside.
    Dodge,
    /// Standing still on purpose: a hop straight up, a hand-off, a goof that
    /// stands.
    Stand,
}

/// What one mover wants this tick. `walk` is a flat direction (zero:
/// stand); a button it sets is `Some`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Proposal {
    pub mover: Mover,
    pub walk: Option<Vec3>,
    pub jump: Option<bool>,
    pub crouch: Option<bool>,
    pub jet: Option<bool>,
}
impl Proposal {
    pub(super) fn walk(mover: Mover, walk: Vec3) -> Self {
        Self {
            mover,
            walk: Some(walk),
            jump: None,
            crouch: None,
            jet: None,
        }
    }
    pub(super) fn buttons(mover: Mover) -> Self {
        Self {
            mover,
            walk: None,
            jump: None,
            crouch: None,
            jet: None,
        }
    }
}

/// Buttons pressed on top of whatever the movers set: a crouch, the jets,
/// and a hop (a goof's, a dodge's, one into a body), which safety lets
/// through only where it comes down on floor along the walk it takes.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Press {
    pub hop: bool,
    pub crouch: bool,
    pub jet: bool,
}

/// The walk and the buttons, before safety.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Controls {
    pub walk: Vec3,
    /// Where its route alone would take it: a walk elsewhere is off it.
    pub routed: Vec3,
    pub jump: bool,
    /// A hop pressed, not yet judged by safety.
    pub hop: bool,
    pub crouch: bool,
    pub jet: bool,
}

/// One decision from every proposal, in [`Mover`] order whatever order
/// they came in, then the presses.
pub(super) fn resolve(proposals: &mut [Proposal], press: Press) -> Controls {
    proposals.sort_by_key(|p| p.mover);
    let mut out = Controls::default();
    for p in proposals.iter() {
        if let Some(walk) = p.walk {
            out.walk = walk;
            if p.mover == Mover::Route {
                out.routed = walk;
            }
        }
        out.jump = p.jump.unwrap_or(out.jump);
        out.crouch = p.crouch.unwrap_or(out.crouch);
        out.jet = p.jet.unwrap_or(out.jet);
    }
    out.hop = press.hop;
    out.crouch |= press.crouch;
    out.jet |= press.jet;
    out
}

/// What the safety stage needs to know of how the bot gets about now.
#[derive(Clone, Copy, Debug)]
pub(super) struct Ground {
    /// It drives a chassis: the chassis has its own checks.
    pub driving: bool,
    /// In water, where a fall is broken.
    pub swimming: bool,
    /// On a jet leg of its route, which flies its own way.
    pub jet_leg: bool,
    /// It may step round a vehicle in its way (on foot, not pushing one).
    pub vehicle_detour: bool,
}

impl Session {
    /// Safety, the last word on the walk, which only takes away: never
    /// through a portal (`passage`) unless its route leads through one, so
    /// footwork, a goof or a push never stumbles in; round a vehicle in the
    /// way; and never off an edge whose fall would hurt it. What it wants
    /// beyond is not worth the fall: it stands at the edge and gets
    /// nowhere, so it plans again. Off its route on its feet it does not
    /// step down where it could not walk back up; in the air (footwork, a
    /// goof, a hop) it does not steer out over a fall that hurts. Its
    /// route's own way through the air, and its jets, it keeps. A hop
    /// pressed goes only where it comes down on floor along the walk it
    /// takes. The walk, whether an edge held it back, and the jump.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bot_safe_walk(
        &self,
        bot: OwnerId,
        body: &Body,
        feet: Vec3,
        state: &crate::player::PlayerState,
        controls: Controls,
        ground: Ground,
        quarry: Option<OwnerId>,
        goal: Option<Vec3>,
    ) -> (Vec3, bool, bool) {
        let mut walk = controls.walk;
        if walk != controls.routed && !ground.driving && walk != Vec3::ZERO {
            let from = feet + Vec3::Y * 0.9;
            let to = from + flat(walk).normalize_or_zero() * PORTAL_REACH;
            if self.simulation.passages().first(from, to).is_some() {
                walk = Vec3::ZERO;
            }
        }
        if ground.vehicle_detour {
            walk = self.bot_vehicle_detour(bot, walk, quarry, goal);
        }
        let off_route = walk != controls.routed;
        let held_back = !ground.driving
            && !ground.swimming
            && (state.grounded || off_route && !controls.jet && !state.jetting)
            && walk != Vec3::ZERO
            && !ground.jet_leg
            && self.bot_fall_ahead(
                bot,
                feet,
                body,
                flat(walk).normalize_or_zero(),
                off_route && state.grounded,
            );
        if held_back {
            walk = Vec3::ZERO;
        }
        let drift = if walk == Vec3::ZERO {
            Vec3::from(state.velocity)
        } else {
            walk * self.peers[&bot].player.tuning().forward
        };
        let hop = controls.hop && super::hop_lands(&self.simulation, feet, drift);
        (walk, held_back, controls.jump || hop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_highest_mover_walks_whatever_order_they_came_in() {
        let mut ps = [
            Proposal::walk(Mover::Dodge, Vec3::X),
            Proposal::walk(Mover::Route, Vec3::Z),
            Proposal::walk(Mover::Goof, Vec3::NEG_X),
        ];
        let c = resolve(&mut ps, Press::default());
        assert_eq!(c.walk, Vec3::X, "a dodge's step beats a goof's walk");
        assert_eq!(c.routed, Vec3::Z);
    }

    #[test]
    fn buttons_are_set_in_order_and_presses_add_on_top() {
        let mut ps = [
            Proposal {
                jump: Some(true),
                crouch: Some(true),
                ..Proposal::walk(Mover::Route, Vec3::Z)
            },
            Proposal {
                jump: Some(false),
                crouch: Some(false),
                jet: Some(true),
                ..Proposal::buttons(Mover::Jet)
            },
        ];
        let c = resolve(&mut ps, Press::default());
        assert!(!c.jump && !c.crouch && c.jet, "the jet leg sets its buttons");
        let c = resolve(
            &mut ps,
            Press {
                crouch: true,
                hop: true,
                ..Default::default()
            },
        );
        assert!(c.crouch, "a press adds to what the movers set");
        assert!(c.hop && !c.jump, "a hop waits for safety's say");
        assert_eq!(c.walk, Vec3::Z, "a mover with no walk leaves it");
    }
}
