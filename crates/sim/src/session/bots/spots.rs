//! Where a fighter stands (`docs/architecture/bots.md`, "Where to stand").
//!
//! A ranged fighter weighs a few places to stand, each a stable option
//! named from where it stands now ([`SPOTS`]): here, a body's width left,
//! right, in toward its enemy or out, a hop up onto a ledge ahead, or a jet
//! up onto one higher. The places come from the walk grid the route
//! planner keeps and the body's measured reach (`nav`, `reach`); nothing
//! new about the map is worked out.
//!
//! Each place has one score, in one unit, harm per second, with no weight
//! of its own ([`Terms::score`]): the damage the best shot from there does
//! (the shot chooser run from that place), over the seconds getting there
//! and firing take, less the share of its own health it would lose
//! meanwhile to the threats it knows of that see the place and to its
//! allies' lines of fire. A healthy bot wants the shot and does not mind
//! being seen; the same harm is a bigger share of less health, so a hurt
//! one takes a worse shot where nobody looks. With no shot from anywhere,
//! it keeps to the place that costs it least. The chooser (`surprise`)
//! picks with its hold rule like every other choice, so a bot does not
//! dance between two places, and the place chosen is a goal its route goes
//! to. Once there, that place is where it stands: the next choice starts
//! from it.
use super::*;
use crate::nav::{Ground, Nav};
use bri_weapons::ActorId;

/// The places a fighter weighs, named from where it stands.
pub(super) const SPOTS: [&str; 7] = ["here", "left", "right", "in", "out", "hop", "jet"];
pub(super) const HERE: u32 = 0;
const LEFT: u32 = 1;
const RIGHT: u32 = 2;
const IN: u32 = 3;
const OUT: u32 = 4;
const HOP: u32 = 5;
const JET: u32 = 6;

/// One place's three quantities: the damage of the best shot from there,
/// the seconds getting there and firing it take, and the share of its
/// health it would lose meanwhile (0 to 1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Terms {
    pub dealt: f32,
    pub seconds: f32,
    pub taken: f32,
}
impl Terms {
    /// Harm dealt per second spent, less the share of health lost.
    pub(super) fn score(self) -> f32 {
        if self.dealt > 0.0 && self.seconds > 0.0 {
            self.dealt / self.seconds * (1.0 - self.taken.clamp(0.0, 1.0))
        } else {
            0.0
        }
    }
}
impl std::fmt::Display for Terms {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(dealt {:.0} in {:.2} s, taken {:.2})",
            self.dealt, self.seconds, self.taken
        )
    }
}

/// The place a fighter heads for, and which of [`SPOTS`] it was.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Anchor {
    pub option: u32,
    pub at: Vec3,
}

/// What the chooser sees for each place: its score; or, when no place has
/// a shot, what is left of its health there, so a bot with nothing to
/// shoot keeps out of sight. Here comes first, so it wins a tie.
pub(super) fn scores(terms: &[Option<Terms>; SPOTS.len()]) -> Vec<(u32, f32)> {
    let shot = terms.iter().flatten().any(|t| t.score() > 0.0);
    terms
        .iter()
        .enumerate()
        .filter_map(|(option, t)| {
            let t = (*t)?;
            Some((
                option as u32,
                if shot {
                    t.score()
                } else {
                    1.0 - t.taken.clamp(0.0, 1.0)
                },
            ))
        })
        .collect()
}

impl Session {
    /// Where `bot`, fighting `seen` with a ranged weapon, heads to stand:
    /// `None` where it stands. Weighed on its planning turn (the shot
    /// chooser's, `combat::Budget`); in between, the place it heads for
    /// holds. `threats` are the enemies it knows of that may shoot it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bot_stand(
        &mut self,
        bot: OwnerId,
        seen: Seen,
        weapon: Weapon,
        threats: &[OwnerId],
        feet: Vec3,
        body: &Body,
        costs: &crate::route::Costs,
        tick: u64,
    ) -> Option<Anchor> {
        let brain = self.bots.brains.get_mut(&bot)?;
        let mut anchor = brain.spot;
        // There, or as near as its route gets: that place is where it
        // stands now.
        if let Some(a) = anchor {
            let there = flat(a.at - feet).length() <= body.width * 0.5
                && (a.at.y - feet.y).abs() <= body.step + 0.5;
            let stuck = brain.settled && brain.plan.is_empty() && brain.search.is_none();
            if there || stuck {
                brain.surprise.arrived(surprise::Domain::Spot, HERE, tick);
                anchor = None;
            }
        }
        if !self.bots.combat_budget.has_turn(bot, tick) {
            return anchor;
        }
        let peer = self.peers.get(&bot)?;
        let health = peer.combat.health;
        let eye = peer.player.eye() - feet;
        let walk_speed = peer.player.tuning().forward.max(1.0);
        // The weapon in hand's cycle: how long a place is stood on for one
        // attack when it has no shot from there.
        // The weapon it fights with's cycle (the one in hand, or the first
        // the shot chooser would take up with its hands empty): how long a
        // place is stood on for one attack when it has no shot from there.
        let cycle = self
            .bot_capability(bot, true)
            .map(|cap| cap.cadence_ticks as f32 / bri_weapons::TICK_HZ as f32)?;
        // Its weapon's band: it fights from no nearer than its near edge
        // (where a blast or a scatter is its own risk), as the behaviour
        // chooser reads it.
        let (near, far) = weapon.band();
        let places = self.bot_places(seen, feet, body, costs, walk_speed, (near, far));
        // The shot from each place: the chooser's own, on a scratch state
        // and a copy of its mind, on its fair share of the shared budget
        // (a place it has no budget left for has no shot this turn).
        let mut budget = self.bots.combat_budget.share();
        let given = budget.allowance();
        let mind = self.bots.brains[&bot].surprise.clone();
        let intents = self.team_intents(bot, tick);
        let reach = self.bot_sight_reach(&self.bots.brains[&bot].kind);
        let weigh = |option: usize, budget: &mut hand_combat::Budget| -> Option<Terms> {
            let (at, travel) = places[option]?;
            let (dealt, fire) = match hand_combat::choose(
                self,
                bot,
                seen,
                at + eye,
                tick,
                &mut hand_combat::State::default(),
                budget,
                &mut mind.clone(),
            ) {
                hand_combat::Decision::Ready(c) | hand_combat::Decision::Charging(c)
                    if c.dealt > 0.0 =>
                {
                    (c.dealt, c.seconds)
                }
                _ => (0.0, cycle),
            };
            let seconds = travel + fire;
            let dealt = if flat(seen.real - at).length() < near {
                0.0
            } else {
                dealt
            };
            // Each threat that sees the place, and each ally whose line of
            // fire crosses it, costs what its weapon deals a second.
            let mut rate = 0.0;
            for &threat in threats {
                let Some(from) = self
                    .peers
                    .get(&threat)
                    .filter(|p| p.combat.alive)
                    .map(|p| p.player.eye())
                else {
                    continue;
                };
                let to = at + eye;
                let shoots = self
                    .bot_capability(threat, false)
                    .filter(|cap| (cap.near..=cap.reach).contains(&from.distance(to)));
                if let Some(cap) = shoots
                    && self
                        .bot_sees(
                            threat,
                            Some(sightlines::Subject::Spot(bot, option as u32)),
                            from,
                            to,
                            reach,
                            sightlines::Urgency::Ordinary,
                        )
                        .is_some()
                {
                    rate += tactics::rate(cap, health);
                }
            }
            for (ally, intent) in &intents {
                if intent
                    .harm
                    .is_some_and(|line| team::inside(&line, at, body.height))
                    && let Some(cap) = self.bot_capability(*ally, false)
                {
                    rate += tactics::rate(cap, health);
                }
            }
            Some(Terms {
                dealt,
                seconds,
                taken: (rate * seconds / health.max(f32::MIN_POSITIVE)).min(1.0),
            })
        };
        let mut terms = [None; SPOTS.len()];
        terms[HERE as usize] = weigh(HERE as usize, &mut budget);
        // Standing with a shot, it weighs the other places only when one
        // could beat it: no place's shot does more than its hardest-hitting
        // weapon at its quickest cycle, even unseen, after the walk there.
        let best = self.bot_best_attack(bot, seen.owner);
        // Against what the chooser makes of the scores: here's at its
        // least after the mind's terms, another's at its most.
        let (low, high) = {
            let brain = &self.bots.brains[&bot];
            brain.surprise.reach(
                &brain.kind.surprise,
                brain.kind.team.copy,
                surprise::Domain::Spot,
                HERE,
            )
        };
        let beaten = |t: Terms| {
            best.is_none_or(|(dealt, fire)| {
                places
                    .iter()
                    .enumerate()
                    .filter(|(o, _)| *o != HERE as usize)
                    .filter_map(|(_, p)| *p)
                    .any(|(_, travel)| dealt / (travel + fire) * high > t.score() * low)
            })
        };
        if anchor.is_some() || terms[HERE as usize].is_none_or(|t| t.score() <= 0.0 || beaten(t)) {
            for (option, term) in terms.iter_mut().enumerate() {
                if option != HERE as usize {
                    *term = weigh(option, &mut budget);
                }
            }
        }
        self.bots.combat_budget.charge(given, &budget);
        let options = scores(&terms);
        let brain = self.bots.brains.get_mut(&bot)?;
        let (cfg, rule) = (brain.kind.surprise.clone(), brain.kind.hold());
        let gate = brain.surprise.gate;
        let chosen = brain.surprise.pick(
            &cfg,
            rule,
            surprise::Choice {
                domain: surprise::Domain::Spot,
                options: &options,
                interrupt: false,
                paused: false,
                must: &[],
                fixed: &[],
            },
            gate,
            tick,
        );
        brain.surprise.spot_terms(terms);
        match (chosen, anchor) {
            (HERE, _) => None,
            // Still on its way to the place it chose.
            (option, Some(a)) if a.option == option => Some(a),
            (option, _) => places[option as usize].map(|(at, _)| Anchor { option, at }),
        }
    }

    /// Each of [`SPOTS`] it can stand on, with the seconds getting there
    /// takes: a body's width aside where the walk grid has floor it walks
    /// straight to, and in or out as far as its weapon's `band` (near, far)
    /// takes it back inside, a body's width at least; a ledge ahead a jump
    /// lands it on, and one higher its jets lift it onto (`reach`).
    fn bot_places(
        &mut self,
        seen: Seen,
        feet: Vec3,
        body: &Body,
        costs: &crate::route::Costs,
        walk_speed: f32,
        (near, far): (f32, f32),
    ) -> [Option<(Vec3, f32)>; SPOTS.len()] {
        let mut places = [None; SPOTS.len()];
        places[HERE as usize] = Some((feet, 0.0));
        let toward = flat(seen.real - feet).normalize_or(Vec3::Z);
        let right = Vec3::new(-toward.z, 0.0, toward.x);
        let simulation = &self.simulation;
        let terrain = |o: Vec3, d: Vec3, r: f32| simulation.terrain_ray(o, d, r);
        let waters = simulation.liquids();
        let ground = Ground {
            physics: &simulation.physics,
            terrain: &terrain,
            passages: simulation.passages(),
            waters: &waters,
            bodies: &[],
            motions: &[],
        };
        let at = match self.bots.navs.iter().position(|(b, _)| b == body) {
            Some(at) => at,
            None => {
                let mut nav = Nav::default();
                nav.begin_tick();
                self.bots.navs.push((*body, nav));
                self.bots.navs.len() - 1
            }
        };
        let nav = &mut self.bots.navs[at].1;
        let mut floor = |at: Vec3| {
            nav.node_at(&ground, body, at)
                .flatten()
                .map(crate::nav::Node::feet)
        };
        let gap = flat(seen.real - feet).length();
        for (option, way, length) in [
            (LEFT, -right, body.width),
            (RIGHT, right, body.width),
            (IN, toward, gap - far),
            (OUT, -toward, near - gap),
        ] {
            places[option as usize] = floor(feet + way * length.max(body.width))
                .filter(|to| ground.walkable(body, feet, *to))
                .map(|to| (to, flat(to - feet).length() / walk_speed));
        }
        // Up ahead: the grid finds a floor within more than a step of the
        // height it is asked at (`Nav::node_at`), so asking a step and
        // again two steps higher at a time misses no height: up to the
        // height a jump lands for the hop, and from there as far as one
        // jump above the enemy's own floor for the jets.
        let ahead = feet + toward * body.width;
        let mut ledge = |from: f32, to: f32, takes: &dyn Fn(Vec3) -> Option<f32>| {
            let mut height = feet.y + from + body.step;
            while height - body.step <= feet.y + to {
                if let Some(at) = floor(ahead.with_y(height))
                    .filter(|at| at.y - feet.y > from && at.y - feet.y <= to)
                    && let Some(seconds) = takes(at)
                {
                    return Some((at, seconds));
                }
                height += 2.0 * body.step;
            }
            None
        };
        places[HOP as usize] = ledge(body.step, body.jump, &|at| {
            Some(flat(at - feet).length() / walk_speed)
        });
        if let Some(jets) = &costs.jets {
            let top = seen.feet.y.max(feet.y) + body.jump - feet.y;
            places[JET as usize] = ledge(body.jump, top, &|at| jets.flight(feet, at));
        }
        places
    }

    /// The most `bot`'s attacks could deal `target` in one go, and the
    /// quickest any of them cycles, in seconds: what no place to stand can
    /// beat. `None` when it has none it reads.
    fn bot_best_attack(&self, bot: OwnerId, target: OwnerId) -> Option<(f32, f32)> {
        let actor = self.weapons.actor(ActorId(bot))?;
        let scale = self.peers.get(&bot)?.player.state().scale;
        let health = self.peers.get(&target)?.combat.health.max(1.0);
        (0..actor.inventory.len())
            .filter_map(|slot| {
                let item = actor.inventory[slot]
                    .as_ref()
                    .and_then(|i| self.weapons.pack.items.get(i))?;
                let image = self.weapons.pack.images.get(&item.image)?;
                let projectile = image
                    .projectile
                    .as_ref()
                    .and_then(|p| self.weapons.pack.projectiles.get(p));
                hand_combat::capability(image, projectile, scale, &self.weapons.pack.projectiles)
            })
            .map(|cap| {
                // A push may send them somewhere that takes all they have.
                let push = if cap.pushes() { health } else { 0.0 };
                (
                    (cap.damage(1.0) + push).min(health),
                    cap.cadence_ticks as f32 / bri_weapons::TICK_HZ as f32,
                )
            })
            .reduce(|a, b| (a.0.max(b.0), a.1.min(b.1)))
    }
    /// What the weapon in `owner`'s hand does (a bot's or a player's), read
    /// as the shot chooser reads it; with `empty_hand`, an empty hand reads
    /// as the first item it could attack with, as the chooser takes one up.
    fn bot_capability(&self, owner: OwnerId, empty_hand: bool) -> Option<tactics::Capability> {
        let actor = self.weapons.actor(ActorId(owner))?;
        let scale = self.peers.get(&owner)?.player.state().scale;
        let of = |slot: usize| {
            let item = actor.inventory[slot]
                .as_ref()
                .and_then(|i| self.weapons.pack.items.get(i))?;
            let image = self.weapons.pack.images.get(&item.image)?;
            let projectile = image
                .projectile
                .as_ref()
                .and_then(|p| self.weapons.pack.projectiles.get(p));
            hand_combat::capability(image, projectile, scale, &self.weapons.pack.projectiles)
        };
        match actor.selected {
            Some(slot) => of(slot),
            None if empty_hand => (0..actor.inventory.len()).find_map(of),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(dealt: f32, seconds: f32, taken: f32) -> Option<Terms> {
        Some(Terms {
            dealt,
            seconds,
            taken,
        })
    }

    #[test]
    fn a_place_scores_harm_per_second_less_the_share_of_health_lost() {
        let t = terms(30.0, 1.5, 0.25).unwrap();
        assert!((t.score() - 30.0 / 1.5 * 0.75).abs() < 1e-5);
        assert_eq!(terms(0.0, 1.0, 0.0).unwrap().score(), 0.0, "no shot");
        assert_eq!(terms(30.0, 1.0, 2.0).unwrap().score(), 0.0, "never below 0");
    }

    #[test]
    fn with_no_shot_anywhere_the_least_exposed_place_wins() {
        let mut all = [None; SPOTS.len()];
        all[HERE as usize] = terms(0.0, 0.5, 0.4);
        all[LEFT as usize] = terms(0.0, 0.8, 0.0);
        let s = scores(&all);
        assert_eq!(s, vec![(HERE, 0.6), (LEFT, 1.0)]);
        // A shot anywhere scores by harm per second instead.
        all[RIGHT as usize] = terms(20.0, 1.0, 0.5);
        let s = scores(&all);
        assert_eq!(s, vec![(HERE, 0.0), (LEFT, 0.0), (RIGHT, 10.0)]);
    }

    #[test]
    fn a_hurt_bot_prefers_the_hidden_place_a_healthy_one_the_quicker_shot() {
        // The same shot from here (seen by a threat dealing 20 a second)
        // and from a step left (unseen, a third of a second's walk).
        let place = |health: f32| {
            let mut all = [None; SPOTS.len()];
            all[HERE as usize] = terms(25.0, 1.0, (20.0 * 1.0 / health).min(1.0));
            all[LEFT as usize] = terms(25.0, 1.0 + 1.0 / 3.0, 0.0);
            let s = scores(&all);
            s.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap().0
        };
        assert_eq!(place(200.0), HERE, "healthy: the quicker shot");
        assert_eq!(place(30.0), LEFT, "hurt: out of sight");
    }

    /// A gunner at the origin and an unarmed enemy straight ahead (+z),
    /// across a wall that hides the enemy from where the gunner stands and
    /// from a step to its -x side, but not from a step to its +x side. The
    /// floor ends just -x of the gunner when `edge`.
    fn behind_a_wall(edge: bool, seed: u64) -> (Session, OwnerId, Seen) {
        fixture(Some(edge), bri_weapons::testing::GUN_ITEM, 12.0, seed)
    }

    /// A bot holding `item` at the origin and an unarmed enemy `ahead` of it
    /// (+z): behind [`behind_a_wall`]'s wall when `wall` (its floor's edge
    /// as given), else on an open floor.
    fn fixture(wall: Option<bool>, item: &str, ahead: f32, seed: u64) -> (Session, OwnerId, Seen) {
        use rapier3d::prelude::*;
        let world = bri_world::World::new("Spots".into(), "fixture".into(), vec![[1.0; 4]]);
        let floor = if wall == Some(true) {
            ColliderBuilder::cuboid(20.0, 0.5, 20.0).translation(Vector::new(19.4, -0.5, 0.0))
        } else {
            ColliderBuilder::cuboid(20.0, 0.5, 20.0).translation(Vector::new(0.0, -0.5, 0.0))
        };
        let mut colliders = vec![floor];
        if wall.is_some() {
            // From x = -6 to x = 0.2, across the line at z = 6.
            colliders.push(
                ColliderBuilder::cuboid(3.1, 4.0, 0.25).translation(Vector::new(-2.9, 4.0, 6.0)),
            );
        }
        let sim =
            crate::simulation::Simulation::new(world, crate::testing::definitions(), colliders)
                .unwrap();
        let mut s = Session::new(sim);
        s.set_weapon_pack(bri_weapons::testing::pack()).unwrap();
        let bot = s
            .join("Gunner".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        let enemy = s
            .join("Target".into(), Vec3::new(0.0, 0.05, ahead), false)
            .unwrap();
        let slot = s.give_item(bot, item).unwrap();
        s.equip_tool(bot, Some(slot)).unwrap();
        let mut kind = BotKind {
            id: "spot-probe".into(),
            ..Default::default()
        };
        kind.surprise.strength = 0.0;
        s.bots
            .brains
            .insert(bot, Brain::new(None, kind, Vec3::ZERO, bot, seed));
        let p = &s.peers[&enemy].player;
        let (eye, feet) = (p.eye(), Vec3::from(p.state().feet));
        let from = s.peers[&bot].player.eye();
        let seen = Seen {
            owner: enemy,
            eye,
            feet,
            aim: sightlines::aim_point(eye, p.state().scale),
            real: feet,
            way: bri_content::passage::Way {
                aim: eye,
                carry: None,
                length: from.distance(eye),
            },
        };
        (s, bot, seen)
    }

    /// The place `bot` chooses on its planning turn at `tick`.
    fn stand(s: &mut Session, bot: OwnerId, seen: Seen, tick: u64) -> Option<Anchor> {
        let feet = Vec3::from(s.peers[&bot].player.state().feet);
        let body = Body::of(s.peers[&bot].player.tuning(), 1.0);
        // Its shot from where it stands first, as the hand chooser takes
        // it each tick: that gives it the planning turn.
        let mut budget = std::mem::take(&mut s.bots.combat_budget);
        budget.begin_tick(tick);
        let mut state = hand_combat::State::default();
        let mut mind = s.bots.brains[&bot].surprise.clone();
        let eye = s.peers[&bot].player.eye();
        let _ = hand_combat::choose(s, bot, seen, eye, tick, &mut state, &mut budget, &mut mind);
        s.bots.combat_budget = budget;
        let costs = crate::route::Costs {
            swim: None,
            jets: None,
        };
        let weapon = s.bot_weapon(bot).unwrap();
        s.bot_stand(bot, seen, weapon, &[seen.owner], feet, &body, &costs, tick)
    }

    #[test]
    fn a_bot_behind_a_wall_steps_aside_to_the_open_side_every_time() {
        for seed in 0..4 {
            let (mut s, bot, seen) = behind_a_wall(false, seed);
            let feet = Vec3::from(s.peers[&bot].player.state().feet);
            let anchor = stand(&mut s, bot, seen, 10).expect("a place with a shot");
            assert!(
                matches!(anchor.option, LEFT | RIGHT) && anchor.at.x > feet.x + 0.5,
                "seed {seed}: {anchor:?}"
            );
            // The open side scored the only shot.
            let terms = s.bots.brains[&bot].surprise.view(&Default::default());
            let spot = terms
                .decisions
                .iter()
                .find(|d| d.domain == "spot")
                .expect("a spot decision");
            assert_eq!(spot.chosen, SPOTS[anchor.option as usize]);
        }
    }

    #[test]
    fn a_bot_never_weighs_a_place_with_no_floor() {
        let (mut s, bot, seen) = behind_a_wall(true, 0);
        let feet = Vec3::from(s.peers[&bot].player.state().feet);
        let body = Body::of(s.peers[&bot].player.tuning(), 1.0);
        let costs = crate::route::Costs {
            swim: None,
            jets: None,
        };
        let places = s.bot_places(seen, feet, &body, &costs, 1.0, (0.0, 20.0));
        for (option, place) in places.iter().enumerate() {
            if let Some((at, _)) = place {
                assert!(at.x > -0.6, "{} at {at} is off the floor", SPOTS[option]);
            }
        }
        assert!(
            places.iter().filter(|p| p.is_some()).count() >= 3,
            "{places:?}"
        );
    }

    #[test]
    fn a_ranged_bot_with_an_enemy_inside_its_band_steps_back_out_of_it() {
        use bri_weapons::testing::{GUN_ITEM, ROCKET_ITEM};
        for item in [ROCKET_ITEM, GUN_ITEM] {
            let (mut s, bot, seen) = fixture(None, item, 2.0, 0);
            let feet = Vec3::from(s.peers[&bot].player.state().feet);
            let (near, _) = s.bot_weapon(bot).unwrap().band();
            assert!(near > 2.0, "{item}: the enemy is inside the band");
            let anchor = stand(&mut s, bot, seen, 10).unwrap_or_else(|| panic!("{item} stood"));
            let away = |at: Vec3| flat(seen.real - at).length();
            assert_eq!(anchor.option, OUT, "{item}: {anchor:?}");
            assert!(
                away(anchor.at) >= near - 0.5 && away(anchor.at) > away(feet),
                "{item}: from {} to {}",
                away(feet),
                away(anchor.at)
            );
        }
    }

    #[test]
    fn a_bot_that_keeps_missing_from_here_is_offered_other_places() {
        let offered = |s: &Session, bot: OwnerId| {
            let brain = &s.bots.brains[&bot];
            brain
                .surprise
                .view(&brain.kind.surprise)
                .decisions
                .iter()
                .find(|d| d.domain == "spot")
                .map_or(0, |d| d.candidates.len())
        };
        let (mut s, bot, seen) = fixture(None, bri_weapons::testing::GUN_ITEM, 8.0, 0);
        // A gun that does not push: where a push sends someone is only
        // known from each place, so a pushing weapon weighs them all.
        let mut pack = s.weapons.pack.as_ref().clone();
        let gun = pack
            .projectiles
            .get_mut(bri_weapons::testing::GUN_PROJECTILE)
            .unwrap();
        (gun.impulse, gun.vertical) = (0.0, 0.0);
        s.weapons.retune(pack).unwrap();
        let brain = s.bots.brains.get_mut(&bot).unwrap();
        brain.kind.surprise.strength = 1.0;
        stand(&mut s, bot, seen, 10);
        assert_eq!(
            offered(&s, bot),
            1,
            "a clear shot from here: only here is weighed"
        );
        let brain = s.bots.brains.get_mut(&bot).unwrap();
        let cfg = brain.kind.surprise.clone();
        for tick in 11..15 {
            brain
                .surprise
                .outcome(&cfg, surprise::Domain::Spot, HERE, false, tick);
        }
        stand(&mut s, bot, seen, 16);
        assert!(
            offered(&s, bot) > 1,
            "missing from here, it weighs other places"
        );
    }
}
