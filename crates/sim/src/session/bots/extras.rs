//! Small extra options, each an ordinary player control the brain already
//! has, all weighed by the kind's one `extras.strength` dial (1 by
//! default, 0 off):
//!
//! - `idle_play`: with no enemy about, pushing a loose body toward a player
//!   in sight and riding along in a teammate's vehicle. The Interact
//!   opportunity scores it as flavour (`interactions.rs`), so anything
//!   with a purpose outranks it.
//! - `crouch`: hurt from range while it holds its ground, it crouches a
//!   moment (damage already scales with crouching).
//! - `dodge`: a projectile whose predicted path (its velocity, its fall and
//!   its splash radius) meets the bot's body makes it jump, jetting if it
//!   can.
//! - `activate`: a door (a brick whose catalog swap a click reverses: the
//!   brick opening and closing itself) standing in its way to its goal is
//!   clicked with the empty hand; now and then, at a natural pause, it
//!   walks to one in sight and clicks it for the fun of it. Nothing else a
//!   click might do (an event row's reset, win, teleport or blast) is ever
//!   pressed: those are a builder's buttons, not a bot's toy.
//! - `hand_weapon`: with a spare attack and an unarmed teammate in sight,
//!   it walks up, faces them and drops the spare their way; the ordinary
//!   contact pickup (or their arming) takes it up.
//!
//! Nothing here reads a content name: only capabilities (damage, splash,
//! swaps, event rows, attack items, seats) and authority (allies, sight,
//! the session's own command checks).
use super::cadence;
use super::*;

/// A hit from farther than this is ranged.
const RANGED: f32 = 5.0;
/// How long a crouch under fire lasts after the last ranged hit.
const CROUCH_TICKS: u64 = 180;
/// How far ahead a projectile's path is followed, and in what steps.
const LOOKAHEAD: f32 = 0.75;
const LOOKAHEAD_STEP: f32 = 1.0 / 60.0;
/// Projectiles farther off than this are not looked at.
const INCOMING: f32 = 40.0;
/// A dodge hop's jets, and the rest after one.
const HOP_TICKS: u64 = 30;
const HOP_REST: u64 = 72;
/// Activation reach (`Command::Activate` reaches five units).
const CLICK_REACH: f32 = 4.5;
/// How far ahead on its way it looks for a brick in the way.
const ROUTE_LOOK: f32 = 2.5;
const CLICK_REST: u64 = 240;
const ROUTE_CLICK_TICKS: u64 = 180;
const FLAVOUR_CLICK_TICKS: u64 = 960;
/// How far off it notices a brick to click for fun.
const FLAVOUR_REACH: f32 = 8.0;
/// How near an unarmed teammate it walks before handing a weapon.
const HAND_NEAR: f32 = 2.6;
const HAND_SIGHT: f32 = 16.0;
const HAND_TICKS: u64 = 1200;
const HAND_REST: u64 = 2400;
/// How long a bot must have seen no objective in play in its game before
/// it plays: the start of a round, before anyone has picked an objective,
/// does not count as calm.
const CALM_TICKS: u64 = 120 * 10;
/// How long an idle-play mark (a player in sight to play toward) holds.
const MARK_TICKS: u64 = 30;
const MARK_SIGHT: f32 = 20.0;
/// Cadence names (`cadence::salt`) of the extras' own checks.
const ROUTE_LOOK_SALT: u64 = 0x4558_0001;
const FLAVOUR_CLICK_SALT: u64 = 0x4558_0002;
const HAND_SALT: u64 = 0x4558_0003;

/// The extra options' own memory. Its random stream is separate from the
/// brain's, so the brain's choices draw exactly as without extras.
#[derive(Clone, Debug, Default)]
pub(super) struct State {
    rng: u64,
    crouch_until: u64,
    hop_until: u64,
    next_hop: u64,
    /// The last projectile judged, and whether it is dodged.
    judged: Option<(u64, bool)>,
    click: Option<Click>,
    next_click: u64,
    /// The tool to take out again after an empty-hand click.
    restore: Option<Option<usize>>,
    /// The teammate it is handing a weapon to, and since when.
    hand: Option<(OwnerId, u64)>,
    next_hand: u64,
    /// The player idle play is aimed toward, and until when.
    mark: Option<(Option<OwnerId>, u64)>,
    /// Since when no objective has been in play in its game.
    calm_since: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct Click {
    brick: BrickId,
    aim: Vec3,
    until: u64,
}

impl State {
    fn roll(&mut self, bot: OwnerId) -> f32 {
        if self.rng == 0 {
            self.rng = 0x9E37_79B9_7F4A_7C15 ^ bot.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1;
        }
        self.rng = self
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// How often an option is taken when it is open: a weight of 1 half the
/// time, 2 or more always.
fn chance(weight: f32) -> f32 {
    (0.5 * weight).clamp(0.0, 1.0)
}

/// What the step knows that the extras need.
pub(super) struct Scene {
    pub behaviour: Behaviour,
    pub enemy_seen: bool,
    pub hurt_by: Option<Knowledge>,
    /// It is not walking anywhere this tick (no waypoint).
    pub holding: bool,
    /// A natural pause (`surprise`): wandering, nothing about.
    pub natural: bool,
    /// Not urgent, not carrying an objective.
    pub calm: bool,
    pub feet: Vec3,
}

/// The controls the extras add.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Extra {
    pub crouch: bool,
    pub jump: bool,
    pub jet: bool,
    pub aim: Option<(f32, f32)>,
    pub stand: bool,
}

/// The earliest time, within `horizon` seconds, a point starting at
/// `from` with `velocity`, falling at `fall` units/s², comes within
/// `radius` of `centre`.
pub(super) fn predicted_hit(
    from: Vec3,
    velocity: Vec3,
    fall: f32,
    centre: Vec3,
    radius: f32,
    horizon: f32,
) -> Option<f32> {
    if !(from.is_finite() && velocity.is_finite() && centre.is_finite()) {
        return None;
    }
    let mut t = 0.0;
    while t <= horizon {
        let at = from + velocity * t - Vec3::Y * (0.5 * fall * t * t);
        if at.distance(centre) <= radius {
            return Some(t);
        }
        t += LOOKAHEAD_STEP;
    }
    None
}

impl Session {
    /// A door: a brick whose click swaps it for another the next click
    /// swaps back (the catalog's swaps), so the click only opens or closes
    /// the brick itself and can be undone. Event rows are never clicked:
    /// what they do (reset, win, teleport, blast) is not the bot's to try.
    pub(super) fn bot_activatable(&self, brick: BrickId) -> bool {
        let Some(placed) = self.simulation.state().bricks.get(&brick) else {
            return false;
        };
        let bri_world::ContentRef::Resolved(id) = &placed.definition else {
            return false;
        };
        let swaps = &self.tool_catalog.swaps;
        swaps.get(id).is_some_and(|swap| {
            [&swap.front, &swap.back].into_iter().all(|to| {
                swaps
                    .get(to)
                    .is_some_and(|back| &back.front == id || &back.back == id)
            })
        })
    }

    /// The player idle play aims toward: the nearest other player it sees.
    pub(super) fn bot_idle_mark(&mut self, bot: OwnerId, tick: u64) -> Option<(OwnerId, Vec3)> {
        let brain = self.bots.brains.get(&bot)?;
        if brain.kind.extras.strength <= 0.0 {
            return None;
        }
        let cached = brain.extras.mark.filter(|(_, until)| tick < *until);
        let mark = match cached {
            Some((mark, _)) => mark,
            None => {
                let eye = self.peers.get(&bot)?.player.eye();
                // Play is for when nothing is at stake: while anyone in its
                // game works an objective, and until it has been calm a
                // while, it does not play with what that game may be about
                // (a ball kicked "toward a friend" could go into its own
                // goal).
                let game = self.game_of(bot);
                let busy = self.bots.brains.iter().any(|(o, b)| {
                    (b.objective.detail().is_some() || b.objective.pursuing())
                        && self.game_of(*o) == game
                });
                let calm = &mut self.bots.brains.get_mut(&bot)?.extras.calm_since;
                *calm = if busy {
                    None
                } else {
                    Some(calm.unwrap_or(tick))
                };
                let at_stake = calm.is_none_or(|since| tick < since + CALM_TICKS);
                let mut near: Vec<(f32, OwnerId)> = self
                    .peers
                    .iter()
                    .filter(|(o, p)| **o != bot && p.combat.alive)
                    .map(|(o, p)| (p.player.eye().distance(eye), *o))
                    .filter(|(d, _)| *d <= MARK_SIGHT)
                    .collect();
                near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let mark = near
                    .into_iter()
                    .take(3)
                    .map(|(_, o)| o)
                    .filter(|_| !at_stake)
                    .find(|o| {
                        self.simulation
                            .sight(eye, self.peers[o].player.eye(), MARK_SIGHT)
                            .is_some()
                    });
                self.bots.brains.get_mut(&bot)?.extras.mark = Some((mark, tick + MARK_TICKS));
                mark
            }
        }?;
        let p = self.peers.get(&mark).filter(|p| p.combat.alive)?;
        Some((mark, Vec3::from(p.player.state().feet)))
    }

    /// The soonest projectile, not its own or an ally's, whose path is
    /// predicted to meet the bot's body (or put it inside its splash).
    fn bot_incoming(&self, bot: OwnerId, feet: Vec3) -> Option<u64> {
        let scale = self.peers.get(&bot)?.player.state().scale;
        let centre = feet + Vec3::Y * scale;
        let mut soonest: Option<(f32, u64)> = None;
        for p in self.weapons.projectiles() {
            let source = p.source.0;
            if p.stuck
                || source == bot
                || self.bot_allies(bot, source)
                || p.position.distance(centre) > INCOMING
                || p.velocity.dot(centre - p.position) <= 0.0
            {
                continue;
            }
            let Some(d) = self.weapons.pack.projectiles.get(&p.definition) else {
                continue;
            };
            if d.damage <= 0.0 && d.explosion.damage <= 0.0 {
                continue;
            }
            let splash = if d.explosion.damage > 0.0 {
                d.explosion.radius.max(0.0)
            } else {
                0.0
            };
            let fall = if d.ballistic { 9.81 * d.gravity } else { 0.0 };
            if let Some(t) = predicted_hit(
                p.position,
                p.velocity,
                fall,
                centre,
                0.9 * scale + splash,
                LOOKAHEAD,
            ) && soonest.is_none_or(|(old, _)| t < old)
            {
                soonest = Some((t, p.id));
            }
        }
        soonest.map(|(_, id)| id)
    }

    /// The extra options for this tick: controls to add, and the commands
    /// (click, drop, tool) a player would give, given here.
    pub(super) fn bot_extras(&mut self, bot: OwnerId, scene: Scene, tick: u64) -> Result<Extra> {
        let mut extra = Extra::default();
        let Some(peer) = self.peers.get(&bot) else {
            return Ok(extra);
        };
        let state = peer.player.state().clone();
        let tuning = peer.player.tuning().clone();
        let eye = peer.player.eye();
        let Some(brain) = self.bots.brains.get(&bot) else {
            return Ok(extra);
        };
        let kind = &brain.kind;
        // One dial (`extras.strength`) weighs every extra option.
        let weights = [kind.extras.strength; 4];
        let swimming = kind.moves == Moves::Swim
            && self
                .simulation
                .liquid_at(state.feet, tuning.stand_height * state.scale)
                .is_some();
        let on_foot = !self.seated(bot) && !swimming;
        let goal = brain.goal.map(|g| g.point(brain.home));
        let [crouch_w, dodge_w, activate_w, hand_w] = weights;
        let feet = scene.feet;
        let look = |at: Vec3| {
            let d = at - eye;
            (yaw_to(d), d.y.atan2(flat(d).length()).clamp(-1.5, 1.5))
        };
        let facing = |at: Vec3| {
            let to = (at - eye).normalize_or_zero();
            state.forward().dot(to) > 0.985
        };

        // Crouch under ranged fire while holding its ground.
        let ranged_hit = scene
            .hurt_by
            .is_some_and(|k| flat(k.at - feet).length() > RANGED);
        // Dodge a projectile predicted to meet it.
        let incoming = (dodge_w > 0.0 && on_foot && tick >= brain.extras.next_hop)
            .then(|| self.bot_incoming(bot, feet))
            .flatten();
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        let st = &mut brain.extras;
        if crouch_w > 0.0
            && ranged_hit
            && (tick < st.crouch_until || st.roll(bot) < chance(crouch_w))
        {
            st.crouch_until = tick + CROUCH_TICKS;
        }
        extra.crouch = tick < st.crouch_until
            && on_foot
            && state.grounded
            && scene.behaviour != Behaviour::Fly
            && (scene.holding || scene.behaviour == Behaviour::Fight);
        if let Some(id) = incoming {
            let dodge = match st.judged {
                Some((judged, dodge)) if judged == id => dodge,
                _ => {
                    let dodge = st.roll(bot) < chance(dodge_w);
                    st.judged = Some((id, dodge));
                    dodge
                }
            };
            if dodge && state.grounded {
                st.hop_until = tick + HOP_TICKS;
                st.next_hop = tick + HOP_REST;
            }
        }
        if tick < st.hop_until {
            extra.jump = state.grounded;
            extra.jet = tuning.can_jet;
            extra.crouch = false;
            // Straight up: a hop that also carried it on could take it
            // off a ledge it was holding back from.
            extra.stand = true;
        }

        // Clicks: one under way, then a tool taken out again.
        let selected = self.weapons.actor(ActorId(bot)).and_then(|a| a.selected);
        let click = self.bots.brains[&bot].extras.click;
        if let Some(c) = click {
            let gone =
                tick > c.until || !self.bot_activatable(c.brick) || !on_foot || scene.enemy_seen;
            if gone {
                self.bots.brains.get_mut(&bot).unwrap().extras.click = None;
            } else if eye.distance(c.aim) <= CLICK_REACH {
                extra.aim = Some(look(c.aim));
                let hit = self
                    .simulation
                    .target_through(eye, state.forward(), 5.0)?
                    .and_then(|(hit, _)| hit.brick);
                if hit == Some(c.brick) {
                    if selected.is_some() {
                        // The empty-hand click: a tool in hand would fire.
                        self.abort_bot_hand_charge(bot)?;
                        let _ = self.equip_tool(bot, None);
                        let st = &mut self.bots.brains.get_mut(&bot).unwrap().extras;
                        st.restore.get_or_insert(selected);
                    } else {
                        let command = |s: &mut Session, c: Command| {
                            let sequence = s.peers.get(&bot).map_or(1, |p| p.last_sequence + 1);
                            let _ = s.command(bot, sequence, c);
                        };
                        command(self, Command::Activate);
                        command(self, Command::ActivateRelease);
                        let st = &mut self.bots.brains.get_mut(&bot).unwrap().extras;
                        st.click = None;
                        st.next_click = tick + CLICK_REST;
                    }
                }
            }
        } else if let Some(restore) = self
            .bots
            .brains
            .get_mut(&bot)
            .unwrap()
            .extras
            .restore
            .take()
            && selected.is_none()
        {
            let _ = self.equip_tool(bot, restore);
        }
        let ready = activate_w > 0.0
            && on_foot
            && !scene.enemy_seen
            && self.bots.brains[&bot].extras.click.is_none()
            && tick >= self.bots.brains[&bot].extras.next_click;
        // A brick in its way to its goal: the click serves the route.
        if ready
            && cadence::beat(bot, ROUTE_LOOK_SALT, tick, 15)
            && let Some(goal) = goal
            && flat(goal - feet).length() > 1.5
        {
            let way = flat(goal - feet).normalize_or_zero();
            let from = feet + Vec3::Y * state.scale;
            if let Some(hit) = self
                .simulation
                .brick_ray(from, way, ROUTE_LOOK, |_, b| b.colliding)?
                && let Some(brick) = hit.brick
                && hit.distance + 0.5 < flat(goal - feet).length()
                && self.bot_activatable(brick)
            {
                self.bots.brains.get_mut(&bot).unwrap().extras.click = Some(Click {
                    brick,
                    aim: hit.position,
                    until: tick + ROUTE_CLICK_TICKS,
                });
            }
        }
        // Now and then at a natural pause: one in sight, for the fun of it.
        let ready = ready && self.bots.brains[&bot].extras.click.is_none();
        if ready
            && scene.natural
            && cadence::beat(bot, FLAVOUR_CLICK_SALT, tick, 240)
            && self.bots.brains.get_mut(&bot).unwrap().extras.roll(bot) < chance(activate_w) * 0.5
        {
            let reach = Vec3::new(FLAVOUR_REACH, 3.0, FLAVOUR_REACH);
            let mut found: Option<(f32, BrickId, Vec3)> = None;
            for brick in self.simulation.bricks_in_box(feet - reach, feet + reach) {
                if !self.bot_activatable(brick) {
                    continue;
                }
                let Some((lo, hi)) = self.simulation.brick_box(brick) else {
                    continue;
                };
                let centre = (lo + hi) * 0.5;
                let far = eye.distance(centre);
                if far > FLAVOUR_REACH * 1.5 || found.is_some_and(|(d, _, _)| d <= far) {
                    continue;
                }
                if let Some((hit, _)) =
                    self.simulation
                        .target_through(eye, centre - eye, FLAVOUR_REACH * 1.5)?
                    && hit.brick == Some(brick)
                {
                    found = Some((far, brick, hit.position));
                }
            }
            if let Some((_, brick, aim)) = found {
                let toward = flat(feet - aim).normalize_or_zero();
                let stand = Vec3::new(aim.x, feet.y, aim.z) + toward * 1.5;
                let brain = self.bots.brains.get_mut(&bot).unwrap();
                brain.set_goal(Some(Goal::Wander(stand)));
                brain.next_wander = brain.next_wander.max(tick + FLAVOUR_CLICK_TICKS);
                brain.extras.click = Some(Click {
                    brick,
                    aim,
                    until: tick + FLAVOUR_CLICK_TICKS,
                });
            }
        }

        // Hand a spare weapon to an unarmed teammate.
        if hand_w > 0.0 && !extra.stand {
            self.bot_hand_weapon(bot, &scene, on_foot, hand_w, eye, &mut extra, tick)?;
            if extra.stand
                && let Some(at) = self.bots.brains[&bot].extras.hand
            {
                let mate = &self.peers[&at.0].player;
                let chest = Vec3::from(mate.state().feet) + Vec3::Y * mate.state().scale * 1.2;
                extra.aim = Some(look(chest));
                if facing(chest) {
                    let slot = self.bot_spare_slot(bot);
                    let st = &mut self.bots.brains.get_mut(&bot).unwrap().extras;
                    st.hand = None;
                    st.next_hand = tick + HAND_REST;
                    if let Some(slot) = slot {
                        self.abort_bot_hand_charge(bot)?;
                        let sequence = self.peers.get(&bot).map_or(1, |p| p.last_sequence + 1);
                        let _ = self.command(bot, sequence, Command::DropTool { slot });
                    }
                }
            }
        }
        Ok(extra)
    }

    /// An attack it can spare: one of two or more, not the one in hand.
    fn bot_spare_slot(&self, bot: OwnerId) -> Option<usize> {
        let a = self.weapons.actor(ActorId(bot))?;
        let scale = self.peers.get(&bot)?.player.state().scale;
        let attacks: Vec<usize> = (0..a.inventory.len())
            .filter(|s| {
                a.inventory[*s]
                    .as_deref()
                    .is_some_and(|i| hand_combat::item_attacks(self, i, scale))
            })
            .collect();
        if attacks.len() < 2 {
            return None;
        }
        attacks.into_iter().rev().find(|s| Some(*s) != a.selected)
    }

    /// A teammate with no attack and room for one.
    fn bot_unarmed(&self, mate: OwnerId) -> bool {
        let (Some(p), Some(a)) = (self.peers.get(&mate), self.weapons.actor(ActorId(mate))) else {
            return false;
        };
        let scale = p.player.state().scale;
        let bites = self
            .bots
            .brains
            .get(&mate)
            .and_then(|b| b.kind.melee.as_ref())
            .is_some_and(|m| m.damage > 0.0);
        p.combat.alive
            && !bites
            && a.inventory.iter().any(Option::is_none)
            && !a
                .inventory
                .iter()
                .flatten()
                .any(|i| hand_combat::item_attacks(self, i, scale))
    }

    #[allow(clippy::too_many_arguments)]
    fn bot_hand_weapon(
        &mut self,
        bot: OwnerId,
        scene: &Scene,
        on_foot: bool,
        weight: f32,
        eye: Vec3,
        extra: &mut Extra,
        tick: u64,
    ) -> Result<()> {
        let open = on_foot
            && scene.calm
            && !scene.enemy_seen
            && scene.behaviour == Behaviour::Wander
            && self.bot_spare_slot(bot).is_some();
        let feet = scene.feet;
        let hand = self.bots.brains[&bot].extras.hand;
        if let Some((mate, since)) = hand {
            let mate_feet = self
                .peers
                .get(&mate)
                .map(|p| Vec3::from(p.player.state().feet));
            let keep = open
                && tick < since + HAND_TICKS
                && self.bot_unarmed(mate)
                && self.bot_allies(bot, mate)
                && !self.seated(mate);
            let Some(mate_feet) = mate_feet.filter(|_| keep) else {
                let st = &mut self.bots.brains.get_mut(&bot).unwrap().extras;
                st.hand = None;
                st.next_hand = tick + HAND_REST;
                return Ok(());
            };
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            if flat(mate_feet - feet).length() > HAND_NEAR {
                if !matches!(brain.goal, Some(Goal::Wander(p)) if flat(p - mate_feet).length() < 1.0)
                {
                    brain.set_goal(Some(Goal::Wander(mate_feet)));
                }
                brain.next_wander = brain.next_wander.max(tick + 120);
            } else {
                brain.set_goal(None);
                brain.next_wander = brain.next_wander.max(tick + 120);
                extra.stand = true;
            }
            return Ok(());
        }
        if !open
            || tick < self.bots.brains[&bot].extras.next_hand
            || !cadence::beat(bot, HAND_SALT, tick, 60)
        {
            return Ok(());
        }
        let mut mates: Vec<(f32, OwnerId)> = self
            .peers
            .iter()
            .filter(|(o, p)| {
                **o != bot
                    && p.combat.alive
                    && !self.seated(**o)
                    && self.bot_allies(bot, **o)
                    && self.bot_unarmed(**o)
            })
            .map(|(o, p)| (p.player.eye().distance(eye), *o))
            .filter(|(d, _)| *d <= HAND_SIGHT)
            .collect();
        mates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mate = mates.into_iter().take(3).map(|(_, o)| o).find(|o| {
            self.simulation
                .sight(eye, self.peers[o].player.eye(), HAND_SIGHT)
                .is_some()
        });
        let st = &mut self.bots.brains.get_mut(&bot).unwrap().extras;
        match mate {
            Some(mate) if st.roll(bot) < chance(weight) => st.hand = Some((mate, tick)),
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_through_the_body_is_a_predicted_hit_and_one_wide_of_it_is_not() {
        let centre = Vec3::new(0.0, 1.0, 0.0);
        let from = Vec3::new(0.0, 1.0, 10.0);
        let at = predicted_hit(from, Vec3::new(0.0, 0.0, -20.0), 0.0, centre, 0.9, 0.75);
        assert!(at.is_some_and(|t| (0.4..0.5).contains(&t)), "{at:?}");
        let wide = predicted_hit(from, Vec3::new(6.0, 0.0, -20.0), 0.0, centre, 0.9, 0.75);
        assert_eq!(wide, None);
        // Too slow to arrive within the look ahead.
        assert_eq!(
            predicted_hit(from, Vec3::new(0.0, 0.0, -5.0), 0.0, centre, 0.9, 0.75),
            None
        );
    }

    #[test]
    fn splash_and_fall_widen_and_bend_the_path() {
        let centre = Vec3::new(0.0, 1.0, 0.0);
        let from = Vec3::new(0.0, 4.0, 10.0);
        let level = Vec3::new(0.0, 0.0, -20.0);
        // Flying level three units over the body it misses...
        assert_eq!(predicted_hit(from, level, 0.0, centre, 0.9, 0.75), None);
        // ...unless its blast reaches that far, or it drops onto it.
        assert!(predicted_hit(from, level, 0.0, centre, 0.9 + 3.0, 0.75).is_some());
        assert!(predicted_hit(from, level, 9.81 * 2.5, centre, 0.9, 0.75).is_some());
    }

    #[test]
    fn a_weight_of_one_is_taken_about_half_the_time_and_zero_never() {
        let mut st = State::default();
        let taken = (0..2000).filter(|_| st.roll(7) < chance(1.0)).count();
        assert!((800..1200).contains(&taken), "{taken}");
        assert!((0..100).all(|_| st.roll(7) >= chance(0.0)));
        assert!((0..100).all(|_| st.roll(7) < chance(2.0)));
    }
}
