//! Small extra options, each an ordinary player control the brain already
//! has:
//!
//! - `idle_play`: with no enemy about, pushing a loose body toward a player
//!   in sight and riding along in a teammate's vehicle. The Interact
//!   opportunity scores it as flavour (`interactions.rs`), so anything
//!   with a purpose outranks it.
//! - `crouch` and `dodge`, how it moves this moment, weighed by the chooser
//!   (`Domain::Move`): hurt from range while it holds its ground, a crouch
//!   (damage already scales with crouching); a projectile whose predicted
//!   path (its velocity, its fall and its splash radius) meets the bot's
//!   body, a dodge, its way chosen too (`Domain::Dodge`): a hop straight
//!   up, a strafe off the shot's line where there is floor, or a jet up
//!   after a crouch's charge, only while its jets have the fuel.
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
/// A dodge's hop or jets, the strafe's step aside, the crouch that charges
/// a jet, and the rest after a dodge.
const HOP_TICKS: u64 = 30;
const STRAFE_TICKS: u64 = 40;
const CHARGE_TICKS: u64 = 10;
const DODGE_REST: u64 = 72;
/// Seconds of jetting a jet dodge takes (`route::Jets::seconds`).
const JET_SECONDS: f32 = 0.5;
/// Activation reach (`Command::Activate` reaches five units).
const CLICK_REACH: f32 = 4.5;
/// How far ahead on its way it looks for a brick in the way.
const ROUTE_LOOK: f32 = 2.5;
const CLICK_REST: u64 = 240;
const ROUTE_CLICK_TICKS: u64 = 180;
const FLAVOUR_CLICK_TICKS: u64 = 960;
/// How often, at a natural pause, it looks for a door in sight.
const DOOR_LOOK: u64 = 60;
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

/// The extra options' own memory.
#[derive(Clone, Debug, Default)]
pub(super) struct State {
    /// Until when it crouches under ranged fire.
    crouch_until: u64,
    /// The dodge under way: its way (`surprise::DODGES`), the side a strafe
    /// steps to, and since when.
    dodge: Option<(u32, Vec3, u64)>,
    next_dodge: u64,
    /// The last projectile judged.
    judged: Option<u64>,
    click: Option<Click>,
    next_click: u64,
    /// The tool to take out again after an empty-hand click.
    restore: Option<Option<usize>>,
    /// A door in sight it saw at a natural pause, where to aim at it, and
    /// until when it counts as seen (`Flavour::Door`).
    door: Option<(BrickId, Vec3, u64)>,
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
    /// Carrying an objective: it keeps to its way (no dodge holds it up).
    pub carrying: bool,
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
    /// Which way to step (a strafe off a shot's line).
    pub direction: Option<Vec3>,
}

/// What keeping on as its footwork has it is worth against a crouch or a
/// dodge (`Domain::Move`). Getting low at a hit from range is worth a
/// little more; a dodge off a shot's path is worth as much and `HARM` of
/// the share of the health it has left the shot would take, so a bullet
/// at full health is dodged about as often as not and a rocket, or a
/// bullet once it is badly hurt, nearly every time. Each way of dodging
/// that is open is worth the same (`Domain::Dodge`): a way grows stale as
/// fast as a goof, so the next dodge goes another way.
const KEEP: f32 = 1.0;
const CROUCH: f32 = 1.05;
const DODGE: f32 = 1.0;
const HARM: f32 = 0.3;

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

    /// Whether a door it may click open (`activate`) stands across the way
    /// from `end` toward `to` within reach: a route ending there is not the
    /// end of the way.
    pub(super) fn bot_opens_way(&self, end: Vec3, to: Vec3) -> bool {
        let way = flat(to - end).normalize_or_zero();
        way != Vec3::ZERO
            && self
                .simulation
                .brick_ray(end + Vec3::Y, way, CLICK_REACH, |_, b| b.colliding)
                .ok()
                .flatten()
                .and_then(|hit| hit.brick)
                .is_some_and(|brick| self.bot_activatable(brick))
    }

    /// The player idle play aims toward: the nearest other player it sees.
    pub(super) fn bot_idle_mark(&mut self, bot: OwnerId, tick: u64) -> Option<(OwnerId, Vec3)> {
        let brain = self.bots.brains.get(&bot)?;
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
    /// predicted to meet the bot's body (or put it inside its splash): its
    /// id, the share of the bot's health it would take, and the flat way
    /// off its line.
    fn bot_incoming(&self, bot: OwnerId, feet: Vec3) -> Option<(u64, f32, Vec3)> {
        let scale = self.peers.get(&bot)?.player.state().scale;
        let centre = feet + Vec3::Y * scale;
        // Of what it has left: a hurt bot clears what a fresh one would not.
        let health = self.peers.get(&bot)?.combat.health.max(1.0);
        let mut soonest: Option<(f32, u64, f32, Vec3)> = None;
        for p in self.weapons.projectiles() {
            let source = p.source.0;
            if p.stuck
                || source == bot
                || p.position.distance(centre) > INCOMING
                || p.velocity.dot(centre - p.position) <= 0.0
                || self.bot_allies(bot, source)
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
            ) && soonest.is_none_or(|(old, _, _, _)| t < old)
            {
                let harm = (d.damage.max(d.explosion.damage) / health).clamp(0.0, 1.0);
                // Square to its flight, toward the side the bot already
                // stands to.
                let across = Vec3::new(-p.velocity.z, 0.0, p.velocity.x).normalize_or_zero();
                let side = if across.dot(centre - p.position) >= 0.0 {
                    across
                } else {
                    -across
                };
                soonest = Some((t, p.id, harm, side));
            }
        }
        soonest.map(|(_, id, harm, side)| (id, harm, side))
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
        let swimming = kind.moves == Moves::Swim
            && self
                .simulation
                .liquid_at(state.feet, tuning.stand_height * state.scale)
                .is_some();
        let on_foot = !self.seated(bot) && !swimming;
        let goal = brain.goal.map(|g| g.point(brain.home));
        let feet = scene.feet;
        let look = |at: Vec3| {
            let d = at - eye;
            (yaw_to(d), d.y.atan2(flat(d).length()).clamp(-1.5, 1.5))
        };
        let facing = |at: Vec3| {
            let to = (at - eye).normalize_or_zero();
            state.forward().dot(to) > 0.985
        };

        // How it moves this moment (`Domain::Move`), chosen as each threat
        // comes: a hit from range while it holds its ground or fights, a
        // crouch or not (a crouch under way lasts while the hits keep
        // coming); a shot predicted to meet it (each judged once), a dodge
        // or not, and which way (`Domain::Dodge`).
        let crouch_open = on_foot
            && state.grounded
            && (scene.holding || scene.behaviour == Behaviour::Fight);
        let ranged_hit = crouch_open
            && scene
                .hurt_by
                .is_some_and(|k| flat(k.at - feet).length() > RANGED);
        let brain = &self.bots.brains[&bot];
        // Carrying an objective it keeps to its way: no dodge holds it up.
        let incoming = (on_foot
            && !scene.carrying
            && state.grounded
            && brain.extras.dodge.is_none()
            && tick >= brain.extras.next_dodge)
            .then(|| self.bot_incoming(bot, feet))
            .flatten()
            .filter(|(id, _, _)| brain.extras.judged != Some(*id));
        // The ways a dodge may go: up where it comes down on floor (a hop,
        // or a jet while its fuel holds a jet's worth), or aside where
        // floor lies under the step, never into an ally's line of fire.
        let velocity = Vec3::from(state.velocity);
        // What its jets can do now, by its kind's `fly` weight and fuel.
        let jets = crate::route::Jets::of(&tuning, state.energy, brain.kind.weight("fly"));
        let ways = incoming.map(|(_, _, side)| {
            use super::surprise::{DODGE_HOP, DODGE_JET, DODGE_STRAFE};
            let up = super::hop_lands(&self.simulation, feet, velocity);
            let fuel = jets.is_some_and(|j| j.seconds >= JET_SECONDS);
            let to = feet + side * tuning.forward * STRAFE_TICKS as f32 / 120.0;
            let aside = super::hop_lands(&self.simulation, feet, side * tuning.forward)
                && !self.bots.claims.intents(tick).any(|(o, i)| {
                    o != bot
                        && self.bot_allies(bot, o)
                        && i.harm.is_some_and(|h| h.holds(to + Vec3::Y, tuning.width))
                });
            let open = |on: bool| if on { DODGE } else { 0.0 };
            [
                (DODGE_HOP, open(up)),
                (DODGE_STRAFE, open(aside)),
                (DODGE_JET, open(up && fuel)),
            ]
        });
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        {
            use super::surprise::{Choice, Domain, MOVE_CROUCH, MOVE_DODGE, MOVE_KEEP};
            // A reflex: hurt or carrying, it still varies how it moves.
            let rule = brain.kind.hold();
            let cfg = &brain.kind.surprise;
            let mut choose = |domain, options: &[(u32, f32)]| {
                let choice = Choice {
                    domain,
                    options,
                    // A threat is answered at once.
                    interrupt: true,
                    paused: false,
                    must: &[],
                    fixed: &[],
                };
                brain
                    .surprise
                    .pick(cfg, rule, choice, Default::default(), tick)
            };
            if ranged_hit {
                let crouching = tick < brain.extras.crouch_until;
                if crouching
                    || choose(Domain::Move, &[(MOVE_KEEP, KEEP), (MOVE_CROUCH, CROUCH)])
                        == MOVE_CROUCH
                {
                    brain.extras.crouch_until = tick + CROUCH_TICKS;
                }
            }
            if let Some(((id, harm, side), ways)) = incoming.zip(ways)
                && ways.iter().any(|(_, s)| *s > 0.0)
            {
                brain.extras.judged = Some(id);
                if choose(Domain::Move, &[(MOVE_KEEP, KEEP), (MOVE_DODGE, DODGE + HARM * harm)])
                    == MOVE_DODGE
                {
                    let way = choose(Domain::Dodge, &ways);
                    brain.extras.dodge = Some((way, side, tick));
                    brain.extras.next_dodge = tick + DODGE_REST;
                }
            }
        }
        extra.crouch = tick < brain.extras.crouch_until && crouch_open;
        let st = &mut self.bots.brains.get_mut(&bot).unwrap().extras;
        if let Some((way, side, since)) = st.dodge {
            use super::surprise::{DODGE_JET, DODGE_STRAFE};
            let t = tick.saturating_sub(since);
            match way {
                DODGE_STRAFE if t < STRAFE_TICKS => extra.direction = Some(side),
                // Crouched a moment, the jump off it carries the jets higher.
                DODGE_JET if t < CHARGE_TICKS => {
                    extra.crouch = true;
                    extra.stand = true;
                }
                DODGE_JET if t < CHARGE_TICKS + HOP_TICKS => {
                    extra.jump = state.grounded;
                    extra.jet = true;
                    extra.crouch = false;
                    extra.stand = true;
                }
                DODGE_STRAFE | DODGE_JET => st.dodge = None,
                _ if t < HOP_TICKS => {
                    extra.jump = state.grounded;
                    extra.jet = jets.is_some();
                    extra.crouch = false;
                    // Straight up: a hop that also carried it on could take
                    // it off a ledge it was holding back from.
                    extra.stand = true;
                }
                _ => st.dodge = None,
            }
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
        let ready = on_foot
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
        // At a natural pause it looks now and then for a door in sight: the
        // goof chooser may walk it up to one and click it (`Flavour::Door`).
        let looking = ready
            && self.bots.brains[&bot].extras.click.is_none()
            && scene.natural
            && cadence::beat(bot, FLAVOUR_CLICK_SALT, tick, DOOR_LOOK);
        if looking {
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
            self.bots.brains.get_mut(&bot).unwrap().extras.door =
                found.map(|(_, brick, aim)| (brick, aim, tick + DOOR_LOOK * 2));
        }

        // Hand a spare weapon to an unarmed teammate.
        if !extra.stand {
            self.bot_hand_weapon(bot, &scene, on_foot, eye, &mut extra, tick)?;
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

    /// The door in sight it last saw at a natural pause, while it stands
    /// and still opens and closes itself.
    pub(super) fn bot_fun_door(&self, bot: OwnerId) -> Option<(BrickId, Vec3)> {
        let brain = self.bots.brains.get(&bot)?;
        let (brick, aim, until) = brain.extras.door?;
        (brain.extras.click.is_none()
            && self.simulation.state().tick < until
            && self.bot_activatable(brick))
        .then_some((brick, aim))
    }

    /// The door goof: walk up to the door in sight and click it, for the
    /// fun of it. The ticks the walk there takes.
    pub(super) fn bot_click_for_fun(&mut self, bot: OwnerId, feet: Vec3, tick: u64) -> Option<u64> {
        let (brick, aim) = self.bot_fun_door(bot)?;
        let walk = self.peers.get(&bot)?.player.tuning().forward.max(1.0);
        let toward = flat(feet - aim).normalize_or_zero();
        let stand = Vec3::new(aim.x, feet.y, aim.z) + toward * 1.5;
        let brain = self.bots.brains.get_mut(&bot)?;
        brain.set_goal(Some(Goal::Wander(stand)));
        brain.next_wander = brain.next_wander.max(tick + FLAVOUR_CLICK_TICKS);
        brain.extras.door = None;
        brain.extras.click = Some(Click {
            brick,
            aim,
            until: tick + FLAVOUR_CLICK_TICKS,
        });
        Some((flat(stand - feet).length() / walk * 120.0) as u64)
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
        eye: Vec3,
        extra: &mut Extra,
        tick: u64,
    ) -> Result<()> {
        // A teammate's spare weapon is teamwork (`team.teamwork`).
        let open = on_foot
            && self.bots.brains[&bot].kind.team.teamwork > 0.0
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
        if let Some(mate) = mate {
            self.bots.brains.get_mut(&bot).unwrap().extras.hand = Some((mate, tick));
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
}
