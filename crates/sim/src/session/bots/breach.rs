//! A blocked pedestrian route may be opened with an ordinary hand weapon.
//! This is a bounded local action, not a search through hypothetical destruction.
//! The normal damage executor owns the hole; navigation owns whether it is usable.
use super::*;

const REACH: f32 = 24.0;
const REST: u64 = 120 * 12;
const DEADLINE: u64 = 120 * 8;
const SHOTS: u8 = 3;

#[derive(Clone, Copy)]
struct Plan {
    brick: BrickId,
    aim: Vec3,
    exit: Vec3,
    goal: Vec3,
    slot: usize,
    safe: f32,
    until: u64,
    shots: u8,
}

#[derive(Default)]
pub(super) struct State {
    plan: Option<Plan>,
    next: u64,
    next_press: u64,
    opened: Option<(Vec3, u64)>,
}

/// Derived diagnostics, never an instruction to a bot.
#[derive(Clone, Copy, Debug, Default)]
pub struct BreachWork {
    pub probes: u64,
    pub attempts: u64,
    pub presses: u64,
    pub openings: u64,
    pub traversals: u64,
    pub abandoned: u64,
}

#[derive(Clone, Copy)]
pub(super) struct Action {
    pub slot: usize,
    pub aim: Vec3,
    pub walk: Vec3,
    pub ready: bool,
}

impl Session {
    pub fn bot_breach_work(&self) -> super::BreachWork {
        self.bots.breach_work
    }

    /// Only read supported, straight, immediate-impact explosive weapons.
    /// Fuses, arcs, charges, scripted attacks and unknown ammo remain unsupported.
    fn breach_tool(&self, bot: OwnerId, slot: usize) -> Option<f32> {
        if !self.bots.brains.get(&bot)?.kind.weapons {
            return None;
        }
        let a = self.weapons.actor(ActorId(bot))?;
        let item = a.inventory.get(slot)?.as_ref()?;
        let image = self
            .weapons
            .pack
            .images
            .get(&self.weapons.pack.items.get(item)?.image)?;
        let p = self
            .weapons
            .pack
            .projectiles
            .get(image.projectile.as_ref()?)?;
        let weapon = self.image_weapon(image, a.frame.scale)?;
        if image.on_fire.is_some()
            || image.charges()
            || weapon.hold
            || image.melee
            || weapon.spread > 0.01
            || p.ballistic
            || p.gravity != 0.0
            || p.arm_ticks != 0
            || p.max_bounces != 0
            || p.speed <= 0.0
            || p.brick.max_volume <= 0.0
            || p.brick.radius < self.peers[&bot].player.tuning().width
            || weapon.reach < REACH
            || weapon.splash > REACH * 0.75
            || hand_combat::rules_rounds(self, bot, image) == Some(0)
            || hand_combat::capability(
                image,
                Some(p),
                a.frame.scale,
                &self.weapons.pack.projectiles,
            )
            .is_none()
        {
            return None;
        }
        if self
            .weapons
            .ammo_on_equip(ActorId(bot), slot)
            .is_some_and(|a| a.rounds == 0)
        {
            return None;
        }
        Some(weapon.splash + self.peers[&bot].player.tuning().width + 1.0)
    }

    pub(super) fn bot_can_breach(&self, bot: OwnerId) -> bool {
        self.weapons.actor(ActorId(bot)).is_some_and(|a| {
            a.inventory
                .iter()
                .take(5)
                .enumerate()
                .any(|(slot, _)| self.breach_tool(bot, slot).is_some())
        })
    }

    /// A failed chase is worth continuing only when an actual nearby,
    /// permitted obstacle can be opened, not merely because it owns a rocket.
    pub(super) fn bot_breachable_way(
        &mut self,
        bot: OwnerId,
        end: Vec3,
        to: Vec3,
        tick: u64,
    ) -> bool {
        if self.bots.breach_budget == 0
            || tick < self.bots.brains[&bot].breach.next
            || !self.bot_can_breach(bot)
        {
            return false;
        }
        self.bots.breach_budget -= 1;
        self.bots.breach_work.probes += 1;
        let origin = end + Vec3::Y * self.peers[&bot].player.tuning().stand_height * 0.55;
        let direction = flat(to - end).normalize_or_zero();
        self.simulation
            .brick_ray(origin, direction, 4.5, |_, b, _| b.colliding)
            .ok()
            .flatten()
            .is_some_and(|hit| {
                hit.brick.is_some_and(|id| {
                    !self.protected_foundation(id)
                        && self.simulation.state().bricks.get(&id).is_some_and(|b| {
                            self.simulation
                                .definitions
                                .get(b)
                                .is_ok_and(|d| !d.indestructible)
                                && self
                                    .damage_scope(bot, hit.position)
                                    .is_some_and(|s| s.allows(b))
                        })
                })
            })
    }

    /// Called before act arbitration. Two local probes per tick globally, at
    /// most four per second per bot. Active actions still yield to combat,
    /// objective tools, swimming, mounts and the ordinary movement safety pass.
    pub(super) fn bot_breach(&mut self, bot: OwnerId, tick: u64, allowed: bool) -> Option<Action> {
        let feet = Vec3::from(self.peers.get(&bot)?.player.state().feet);
        let allowed = allowed && self.bots.brains.get(&bot)?.kind.weapons;
        let reached = (self.peers[&bot].player.tuning().width * 0.5).max(0.5);
        let state = &mut self.bots.brains.get_mut(&bot)?.breach;
        if let Some((exit, until)) = state.opened {
            if flat(exit - feet).length() < reached {
                self.bots.breach_work.traversals += 1;
                state.opened = None;
            } else if tick >= until {
                state.opened = None;
            }
        }
        if !allowed {
            if state.plan.take().is_some() {
                state.next = tick + REST;
                self.bots.breach_work.abandoned += 1;
            }
            return None;
        }
        let brain = &self.bots.brains[&bot];
        let Some(goal) = brain.goal.map(|g| g.point(brain.home)) else {
            let state = &mut self.bots.brains.get_mut(&bot)?.breach;
            state.plan = None;
            state.next = tick + REST;
            return None;
        };
        let eye = self.peers[&bot].player.eye();
        let old = brain.breach.plan;
        if let Some(p) = old {
            if tick >= p.until
                || flat(p.goal - goal).length() > 3.0
                || self.breach_tool(bot, p.slot).is_none()
            {
                let state = &mut self.bots.brains.get_mut(&bot)?.breach;
                state.plan = None;
                state.next = tick + REST;
                self.bots.breach_work.abandoned += 1;
                return None;
            }
        } else if tick < brain.breach.next
            || !brain.partial_route
            || flat(goal - feet).length() < 3.0
        {
            return None;
        }
        let probe = tick >= self.bots.brains[&bot].breach.next && self.bots.breach_budget > 0;
        if probe {
            self.bots.breach_budget -= 1;
            self.bots.breach_work.probes += 1;
            let state = &mut self.bots.brains.get_mut(&bot)?.breach;
            state.next = tick + 30;
            state.plan = None;
            // Do not shoot through a fixed map surface to reach a brick.
            let aim = old.map_or(Vec3::new(goal.x, eye.y - 0.5, goal.z), |p| p.aim);
            let direction = (aim - eye).normalize_or_zero();
            let reach = eye.distance(aim).min(REACH);
            let hit = self
                .simulation
                .brick_ray(eye, direction, reach, |_, b, _| b.colliding)
                .ok()
                .flatten();
            if let Some(p) = old.filter(|_| hit.is_none()) {
                // The original may have split without leaving a walkable hole.
                // A clear eye ray alone is not proof of a usable opening.
                if self.breach_exit_clear(bot, p.exit, true) {
                    let brain = self.bots.brains.get_mut(&bot)?;
                    brain.breach.plan = None;
                    brain.breach.opened = Some((p.exit, tick + REST * 2));
                    brain.breach.next = tick + REST;
                    brain.plan.clear();
                    brain.search = None;
                    brain.settled = false;
                    brain.out_of_reach = None;
                    self.bots.breach_work.openings += 1;
                    return None;
                }
            }
            if let Some(hit) = hit.filter(|h| h.normal.y.abs() < 0.6 && h.distance > 0.5)
                && let Some(id) = hit.brick
                && let Some(b) = self.simulation.state().bricks.get(&id)
                && !self.protected_foundation(id)
                && self
                    .simulation
                    .definitions
                    .get(b)
                    .is_ok_and(|d| !d.indestructible)
                && self
                    .damage_scope(bot, hit.position)
                    .is_some_and(|s| s.allows(b))
                && self
                    .world_ray(eye, direction, hit.distance)
                    .is_none_or(|d| d + 0.1 >= hit.distance)
            {
                let tool = old.map(|p| (p.slot, p.safe)).or_else(|| {
                    (0..5).find_map(|slot| self.breach_tool(bot, slot).map(|safe| (slot, safe)))
                });
                if let Some((slot, safe)) = tool {
                    let exit = old.map_or(
                        Vec3::new(hit.position.x, feet.y, hit.position.z)
                            + flat(direction).normalize_or_zero() * 2.0,
                        |p| p.exit,
                    );
                    if self.breach_exit_clear(bot, exit, false) {
                        // One assembly at a time, even when several bots see it.
                        let claimed = self.bots.brains.iter().any(|(o, b)| {
                            *o != bot
                                && b.breach
                                    .plan
                                    .is_some_and(|p| tick < p.until && p.brick == id)
                        });
                        if !claimed {
                            self.bots.brains.get_mut(&bot)?.breach.plan = Some(Plan {
                                brick: id,
                                aim: hit.position,
                                exit,
                                goal,
                                slot,
                                safe,
                                until: old.map_or(tick + DEADLINE, |p| p.until),
                                shots: old.map_or(0, |p| p.shots),
                            });
                            if old.is_none() {
                                self.bots.breach_work.attempts += 1;
                            }
                        }
                    }
                }
            }
            if old.is_some() && self.bots.brains[&bot].breach.plan.is_none() {
                self.bots.brains.get_mut(&bot)?.breach.next = tick + REST;
                self.bots.breach_work.abandoned += 1;
            }
        }
        let p = self.bots.brains[&bot].breach.plan?;
        let away = flat(feet - p.aim).normalize_or_zero();
        let ready = eye.distance(p.aim) >= p.safe;
        Some(Action {
            slot: p.slot,
            aim: p.aim,
            walk: if ready { Vec3::ZERO } else { away },
            ready: ready && p.shots < SHOTS && tick >= self.bots.brains[&bot].breach.next_press,
        })
    }

    fn breach_exit_clear(&self, bot: OwnerId, exit: Vec3, through: bool) -> bool {
        let body = Body::of(self.peers[&bot].player.tuning(), 1.0);
        let s = &self.simulation;
        let terrain = |o, d, r| s.terrain_ray(o, d, r);
        let waters = s.liquids();
        let ground = Ground {
            physics: &s.physics,
            terrain: &terrain,
            passages: s.passages(),
            waters: &waters,
            bodies: &[],
            motions: &[],
            solid: &|_, _| true,
        };
        ground.fits(&body, exit + Vec3::Y * 0.05, false)
            && (!through
                || ground.walkable(
                    &body,
                    Vec3::from(self.peers[&bot].player.state().feet),
                    exit,
                ))
            && self
                .world_ray(exit + Vec3::Y * body.step, Vec3::NEG_Y, body.step + 0.3)
                .is_some()
    }

    pub(super) fn bot_breach_pressed(&mut self, bot: OwnerId, tick: u64) {
        let cycle = self
            .weapons
            .image_state(ActorId(bot), 0)
            .map_or(120, |(image, _)| hand_combat::cadence(image).max(30));
        self.bots.brains.get_mut(&bot).unwrap().breach.next_press = tick + u64::from(cycle);
        if let Some(p) = self
            .bots
            .brains
            .get_mut(&bot)
            .and_then(|b| b.breach.plan.as_mut())
        {
            p.shots += 1;
            self.bots.breach_work.presses += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d::prelude::*;

    fn corridor(indestructible: bool) -> (Session, OwnerId, BrickId) {
        let mut definitions = crate::testing::definitions();
        definitions.entries.insert(
            "wall".into(),
            crate::testing::definition(
                "wall",
                [10, 1],
                25,
                crate::definitions::Special::None,
                indestructible,
            ),
        );
        let sim = crate::simulation::Simulation::new(
            bri_world::World::new("Breach".into(), "fixture".into(), vec![[1.0; 4]]),
            definitions,
            vec![
                ColliderBuilder::cuboid(3.0, 0.5, 25.0).translation(Vec3::new(0.0, -0.5, 10.0)),
                ColliderBuilder::cuboid(1.0, 10.0, 30.0).translation(Vec3::new(-4.0, 10.0, 10.0)),
                ColliderBuilder::cuboid(1.0, 10.0, 30.0).translation(Vec3::new(4.0, 10.0, 10.0)),
            ],
        )
        .unwrap();
        let mut s = Session::new(sim);
        s.set_lan_host(true);
        let mut pack = bri_weapons::testing::pack();
        // Match a straight rocket's blast-only brick policy; direct=false
        // must not be mistaken for an inability to destroy a wall.
        pack.projectiles
            .get_mut(bri_weapons::testing::ROCKET_PROJECTILE)
            .unwrap()
            .brick
            .direct = false;
        s.set_weapon_pack(pack).unwrap();
        let bot = s.join("Builder".into(), Vec3::Y * 0.05, true).unwrap();
        let Reply::Planted(wall) = s
            .command(
                bot,
                1,
                Command::Plant {
                    definition: "wall".into(),
                    position: [0.0, 2.5, 10.25],
                    quarter_turns: 0,
                    color: 0,
                },
            )
            .unwrap()
        else {
            panic!("wall");
        };
        let slot = s.give_item(bot, bri_weapons::testing::ROCKET_ITEM).unwrap();
        s.equip_tool(bot, Some(slot)).unwrap();
        let mut kind = BotKind {
            sight: 0.0,
            wander_radius: 0.0,
            ..Default::default()
        };
        kind.surprise.strength = 0.0;
        kind.behaviours.insert("fly".into(), 0.0);
        kind.behaviours.insert("respawn".into(), 0.0);
        kind.behaviours.insert("return".into(), 0.0);

        let mut brain = Brain::new(None, kind, Vec3::ZERO, bot, 0);
        brain.intent = Some(intents::Held {
            intent: bri_package_runtime::ops::BotIntent::Patrol {
                points: vec![[0.0, 0.0, 18.0]],
                radius: 30.0,
            },
            next: 0,
            until: u64::MAX,
        });
        s.bots.brains.insert(bot, brain);
        s.weapons.set_bot(ActorId(bot), true).unwrap();
        (s, bot, wall)
    }

    #[test]
    fn a_blocked_patrol_uses_real_rockets_and_walks_through_the_opening() {
        let (mut s, bot, _) = corridor(false);
        for _ in 0..120 * 25 {
            s.step().unwrap();
            s.take_cues();
            if s.bot_breach_work().traversals > 0 {
                break;
            }
        }
        let work = s.bot_breach_work();
        eprintln!(
            "breach {work:?}, feet {:?}, why {:?}",
            s.peers[&bot].player.state().feet,
            s.bot_why()
        );
        assert!(
            work.attempts > 0 && work.presses > 0,
            "no ordinary attack: {work:?}"
        );
        assert!(
            work.openings > 0 && work.traversals > 0,
            "no useful passage: {work:?}"
        );
        assert_eq!(s.bot_step_failures().0, 0);
        assert!(work.presses <= work.attempts * u64::from(SHOTS));
    }

    #[test]
    fn protected_walls_and_disabled_damage_are_not_breach_targets() {
        for protected in [false, true] {
            let (mut s, bot, wall) = corridor(protected);
            if !protected {
                s.command(
                    bot,
                    2,
                    Command::MiniGame(MiniGameRequest::Create {
                        color: 0,
                        settings: bri_minigames::Settings {
                            brick_damage: false,
                            loadout: Default::default(),
                            ..Default::default()
                        },
                    }),
                )
                .unwrap();
                let slot = s.give_item(bot, bri_weapons::testing::ROCKET_ITEM).unwrap();
                s.equip_tool(bot, Some(slot)).unwrap();
                assert!(s.bot_can_breach(bot));
            }
            for _ in 0..120 * 4 {
                s.step().unwrap();
                s.take_cues();
            }
            assert_eq!(s.bot_breach_work().presses, 0);
            assert!(!s.simulation.state().bricks[&wall].damage_hidden);
        }
    }
}
