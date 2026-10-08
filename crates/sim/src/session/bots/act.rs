//! The act stage: the one place a bot's choices become its walk and its
//! buttons this tick.
//!
//! The chooser has picked a behaviour; its route, its stance in a fight, a
//! jet leg, a swim, a dodge, a goof and the like each
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
//!
//! The look is decided the same way: each that wants the bot's eyes
//! proposes a [`Look`], and the highest [`Looker`] has them. What the stage
//! did (who walked, who looked, who has the trigger, an edge held back,
//! stalls and replans) is kept as [`Acted`] for the readout (F3).
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
    /// ranged strafe round the place it fights from (`spots`).
    Stance,
    /// A jet leg of its route, flying itself.
    Jet,
    /// Afloat, or a swimmer rising and diving.
    Swim,
    /// A goof's walk (a circle, a detour, up to someone).
    Goof,
    /// A dodge's step aside.
    Dodge,
    /// Standing still on purpose: a hop straight up, a hand-off, a goof that
    /// stands.
    Stand,
}

impl Mover {
    pub(super) fn name(self) -> &'static str {
        match self {
            Mover::Route => "route",
            Mover::Push => "push",
            Mover::Stance => "stance",
            Mover::Jet => "jet",
            Mover::Swim => "swim",
            Mover::Goof => "goof",
            Mover::Dodge => "dodge",
            Mover::Stand => "stand",
        }
    }
}

/// Who has its eyes, lowest priority first: a later looker's look replaces
/// an earlier one's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Looker {
    /// Where it already looks.
    #[default]
    Hold,
    /// Sweeping the spot it searches.
    Sweep,
    /// The way it walks.
    Route,
    /// Someone it just noticed.
    Glance,
    /// The enemy it fights.
    Target,
    /// What its objective works.
    Objective,
    /// The way it carries something, and the swing that flings it.
    Carry,
    /// Down at its feet (a goof).
    Down,
    /// A goof's or an extra's gesture.
    Gesture,
}
impl Looker {
    pub(super) fn name(self) -> &'static str {
        match self {
            Looker::Hold => "hold",
            Looker::Sweep => "sweep",
            Looker::Route => "route",
            Looker::Glance => "glance",
            Looker::Target => "target",
            Looker::Objective => "objective",
            Looker::Carry => "carry",
            Looker::Down => "down",
            Looker::Gesture => "gesture",
        }
    }
}

/// Where one looker wants its eyes: a yaw and a pitch.
#[derive(Clone, Copy, Debug)]
pub(super) struct Look {
    pub by: Looker,
    pub yaw: f32,
    pub pitch: f32,
}

/// The look of the highest looker.
pub(super) fn look(looks: &[Look]) -> Look {
    looks.iter().copied().max_by_key(|l| l.by).unwrap_or(Look {
        by: Looker::Hold,
        yaw: 0.0,
        pitch: 0.0,
    })
}

/// Who has the trigger this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Trigger {
    Fight,
    Objective,
    Goof,
}
impl Trigger {
    fn name(self) -> &'static str {
        match self {
            Trigger::Fight => "fight",
            Trigger::Objective => "objective",
            Trigger::Goof => "goof",
        }
    }
}

/// What the act stage did last tick, for the readout.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Acted {
    /// Who set the walk (none: it stood).
    pub walk: Option<Mover>,
    pub look: Looker,
    pub trigger: Option<Trigger>,
    /// Safety held its walk back at an edge.
    pub held_back: bool,
    /// Windows gone nowhere in a row, and plans made again.
    pub stalls: u32,
    pub replans: u32,
}
impl Acted {
    /// One readout line: `Acts: walk route, look target, trigger fight;
    /// held at an edge; stalled 1, replanned 2`.
    pub(super) fn line(&self) -> String {
        let mut line = format!(
            "Acts: walk {}, look {}, trigger {}",
            self.walk.map_or("none", Mover::name),
            self.look.name(),
            self.trigger.map_or("none", Trigger::name)
        );
        if self.held_back {
            line.push_str("; held at an edge");
        }
        if self.stalls > 0 || self.replans > 0 {
            line.push_str(&format!(
                "; stalled {}, replanned {}",
                self.stalls, self.replans
            ));
        }
        line
    }
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
/// a hop (a goof's, a dodge's, one into a body), which safety lets through
/// only where it comes down on floor along the walk it takes, and the
/// stall judge's jump in place (over what its shins catch, off a body it
/// stands on), which goes as it is: its walk is already checked.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Press {
    pub hop: bool,
    pub jump: bool,
    pub crouch: bool,
    pub jet: bool,
}

/// The walk and the buttons, before safety.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Controls {
    pub walk: Vec3,
    /// Who set the walk.
    pub by: Option<Mover>,
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
            out.by = Some(p.mover);
            if p.mover == Mover::Route {
                out.routed = walk;
            }
        }
        out.jump = p.jump.unwrap_or(out.jump);
        out.crouch = p.crouch.unwrap_or(out.crouch);
        out.jet = p.jet.unwrap_or(out.jet);
    }
    out.hop = press.hop;
    out.jump |= press.jump;
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
    /// way; never into a live blast that would catch it, and out of one it
    /// stands in; and never off an edge whose fall would hurt it. What it wants
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
        // Never into a live blast, a grenade lying where it waits to go off
        // its own included: it stands until it has; in one, it walks
        // straight out. Decided first, so the way out meets the same
        // portal, vehicle and edge checks as any other walk.
        if !ground.driving && !ground.swimming {
            let toward = flat(walk).normalize_or_zero();
            walk = match self.bot_blast_walk(bot, feet, body, toward) {
                Some(way) if way == toward => walk,
                Some(way) => way,
                None => Vec3::ZERO,
            };
        }
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
        let tuning = self.peers[&bot].player.tuning();
        let drift = if walk == Vec3::ZERO {
            Vec3::from(state.velocity)
        } else {
            walk * tuning.forward
        };
        let hop = controls.hop && super::hop_lands(&self.simulation, feet, drift, tuning, 0.0);
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
        assert!(
            !c.jump && !c.crouch && c.jet,
            "the jet leg sets its buttons"
        );
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

    /// A live rocket lying ahead of a bot, its blast reaching 5: from
    /// outside it a step in is not taken and one away is; standing inside
    /// it, the walk turns straight out. One that cannot hurt it (outside a
    /// mini-game nothing does) is not avoided.
    #[test]
    fn a_bot_keeps_out_of_a_live_blast_and_walks_out_of_one() {
        use rapier3d::prelude::*;
        let world = bri_world::World::new("Blast".into(), "fixture".into(), vec![[1.0; 4]]);
        let floor =
            ColliderBuilder::cuboid(20.0, 0.5, 20.0).translation(Vector::new(0.0, -0.5, 0.0));
        let sim =
            crate::simulation::Simulation::new(world, crate::testing::definitions(), vec![floor])
                .unwrap();
        let mut s = Session::new(sim);
        s.set_weapon_pack(bri_weapons::testing::pack()).unwrap();
        let bot = s
            .join("Walker".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        let thrower = s
            .join("Thrower".into(), Vec3::new(9.0, 0.05, 0.0), false)
            .unwrap();
        s.weapons
            .spawn(
                bri_weapons::testing::ROCKET_PROJECTILE,
                bri_weapons::ActorId(thrower),
                Vec3::new(0.0, 0.5, 6.0),
                Vec3::ZERO,
                1.0,
            )
            .unwrap();
        let body = crate::nav::Body::of(s.peers[&bot].player.tuning(), 1.0);
        let edge = Vec3::ZERO;
        assert_eq!(
            s.bot_blast_walk(bot, edge, &body, Vec3::Z),
            Some(Vec3::Z),
            "outside a game it cannot hurt"
        );
        super::super::harm::one_game(&mut s, thrower, bot);
        assert_eq!(s.bot_blast_walk(bot, edge, &body, Vec3::Z), None, "into it");
        assert_eq!(
            s.bot_blast_walk(bot, edge, &body, Vec3::NEG_Z),
            Some(Vec3::NEG_Z),
            "away"
        );
        let inside = Vec3::new(0.0, 0.0, 4.0);
        let out = s.bot_blast_walk(bot, inside, &body, Vec3::X).unwrap();
        assert!(out.z < -0.9, "straight out: {out}");
    }

    /// A live rocket lying 3.5 units in from a bot standing at a deck's
    /// edge, a drop that kills past it: the way out of the blast is over
    /// the edge, which the edge check refuses, so it stands; with floor
    /// past the edge it walks out.
    #[test]
    fn the_way_out_of_a_blast_meets_the_edge_check() {
        use rapier3d::prelude::*;
        let walk_out = |drop: bool| {
            let world = bri_world::World::new("Ledge".into(), "fixture".into(), vec![[1.0; 4]]);
            let mut colliders = vec![
                ColliderBuilder::cuboid(10.0, 0.5, 10.0).translation(Vector::new(-9.0, -0.5, 0.0)),
            ];
            colliders.push(if drop {
                ColliderBuilder::cuboid(40.0, 0.5, 40.0).translation(Vector::new(0.0, -30.5, 0.0))
            } else {
                ColliderBuilder::cuboid(40.0, 0.5, 40.0).translation(Vector::new(0.0, -0.5, 0.0))
            });
            let sim =
                crate::simulation::Simulation::new(world, crate::testing::definitions(), colliders)
                    .unwrap();
            let mut s = Session::new(sim);
            s.set_weapon_pack(bri_weapons::testing::pack()).unwrap();
            let bot = s
                .join("Walker".into(), Vec3::new(0.5, 0.05, 0.0), false)
                .unwrap();
            let thrower = s
                .join("Thrower".into(), Vec3::new(-8.0, 0.05, 6.0), false)
                .unwrap();
            super::super::harm::one_game(&mut s, thrower, bot);
            for _ in 0..30 {
                s.step().unwrap();
            }
            s.weapons
                .spawn(
                    bri_weapons::testing::ROCKET_PROJECTILE,
                    bri_weapons::ActorId(thrower),
                    Vec3::new(-3.0, 0.5, 0.0),
                    Vec3::ZERO,
                    1.0,
                )
                .unwrap();
            let state = s.peers[&bot].player.state().clone();
            let feet = Vec3::from(state.feet);
            let body = crate::nav::Body::of(s.peers[&bot].player.tuning(), 1.0);
            let ground = Ground {
                driving: false,
                swimming: false,
                jet_leg: false,
                vehicle_detour: false,
            };
            s.bot_safe_walk(
                bot,
                &body,
                feet,
                &state,
                Controls::default(),
                ground,
                None,
                None,
            )
            .0
        };
        assert_eq!(walk_out(true), Vec3::ZERO, "not off the edge");
        assert!(walk_out(false).x > 0.9, "out of the blast onto the floor");
    }
}
