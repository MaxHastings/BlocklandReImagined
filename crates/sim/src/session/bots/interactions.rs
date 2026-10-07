//! Environmental opportunities and short, exclusive commitments. The brain
//! still chooses, paths and aims; this module discovers object capabilities
//! and translates the chosen action into ordinary player controls.
use super::*;
use bri_vehicles::{Family, VehicleId, VehicleSnapshot};
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;

const DISCOVER: f32 = 24.0;
/// What each unit of the walk to an opportunity takes off its utility.
const TRAVEL_COST: f32 = 0.002;
use super::claims::GIVE_UP_TICKS;
/// A driver reaches a goal at least this many degrees off the hull's
/// heading in reverse, but a pursued target only within `REVERSE_DISTANCE`;
/// a farther one is turned toward.
const REVERSE_DEGREES: f32 = 103.0;
const REVERSE_DISTANCE: f32 = 16.0;
pub(super) const CREW_WAIT: u64 = 360;
const OBJECTS_PER_BOT: usize = 8;
const LOOKAHEAD_POINTS: usize = 24;
/// How far past its own half-length a driver that only carries its bot to
/// an enemy stops and gets out: about where the bot fights from on foot.
const LEAVE_REACH: f32 = 3.0;
/// Idle play (`extras`, no enemy about) scores between wandering and
/// walking home, so any purpose outranks it.
const IDLE: f32 = 0.2;
/// An idle push stops this far short of the player it plays toward.
const IDLE_SHORT: f32 = 3.5;

#[derive(Clone, Copy)]
pub(super) struct PushApproach {
    pub point: Vec3,
    pub pushing: bool,
}

/// Static navigation cannot see this moving body. An intermediate arc keeps
/// ordinary approach chords outside its expanded hull before pushing from the
/// actual rear axis. Work is constant and collision remains the motor's owner.
pub(super) fn push_approach(
    feet: Vec3,
    centre: Vec3,
    toward: Vec3,
    radius: f32,
    width: f32,
) -> Option<PushApproach> {
    if !feet.is_finite()
        || !centre.is_finite()
        || !toward.is_finite()
        || !radius.is_finite()
        || radius <= 0.0
        || !width.is_finite()
        || width <= 0.0
    {
        return None;
    }
    let toward = flat(toward).try_normalize()?;
    let back = -toward;
    let offset = flat(feet - centre);
    let distance = offset.length();
    if !distance.is_finite() {
        return None;
    }
    let along = offset.dot(back);
    let lateral = (offset - back * along).length();
    let path_radius = radius + 0.35;
    let pushing = along > 0.0 && lateral < (width * 0.2).max(0.1) && distance < path_radius + 0.5;
    let point = if pushing {
        centre + toward * (radius + 1.5)
    } else {
        let radial = offset.try_normalize().unwrap_or(back);
        if distance <= radius {
            centre + radial * path_radius
        } else {
            let angle = radial.dot(back).clamp(-1.0, 1.0).acos();
            let safe = ((radius / distance).clamp(0.0, 1.0).acos()
                + (radius / path_radius).clamp(0.0, 1.0).acos())
                * 0.8;
            let turn = angle.min(safe);
            let sign = if radial.x * back.z - radial.z * back.x >= 0.0 {
                1.0
            } else {
                -1.0
            };
            let side = Vec3::new(-radial.z, 0.0, radial.x);
            centre + (radial * turn.cos() + side * (sign * turn.sin())) * path_radius
        }
    };
    point.is_finite().then_some(PushApproach { point, pushing })
}

fn object_centre(v: &VehicleSnapshot, d: &bri_vehicles::Definition) -> Vec3 {
    Vec3::from(v.transform.position)
        + glam::Quat::from_array(v.transform.rotation)
            * (Vec3::from(d.bounds_min) + Vec3::from(d.bounds_max))
            * (v.scale * 0.5)
}
use super::claims::Resource;
/// How near the spot a bot makes for another body contests it rather than
/// stands in its way (`Session::bot_walk_direction`).
const CONTESTED: f32 = 3.0;

#[derive(Clone, Copy)]
pub(super) struct Opportunity {
    pub(super) resource: Resource,
    pub point: Vec3,
    pub utility: f32,
}

/// The space a shot from `origin` at `target` sweeps: on `past` beyond,
/// `splash` wide, widening by `spread` radians; none for a shot too short
/// to judge (`Session::bot_fire_clear`).
pub(super) fn shot_space(
    origin: Vec3,
    target: Vec3,
    splash: f32,
    past: f32,
    spread: f32,
) -> Option<super::claims::Space> {
    let delta = target - origin;
    let length = delta.length();
    (length >= 0.01).then(|| super::claims::Space {
        from: origin,
        to: origin + delta / length * (length + past),
        radius: splash,
        spread: spread.tan(),
    })
}

impl Session {
    /// A passenger may have boarded the reachable lower seat of a tall vehicle.
    /// Fill its useful empty role through the same seat keys players use.
    pub(super) fn promote_bot_seat(&mut self, bot: OwnerId) -> Result<()> {
        if !self.is_alive(bot)
            || self.bots.brains.get(&bot).is_none_or(|b| {
                b.resting || b.kind.weight("interact") <= 0.0
            })
        {
            return Ok(());
        }
        let Some((vehicle, seat)) = self.mounted(bot) else {
            return Ok(());
        };
        let Some(world) = &self.vehicles.world else {
            return Ok(());
        };
        let Some(v) = world.vehicle_snapshot(&self.simulation.physics, VehicleId(vehicle)) else {
            return Ok(());
        };
        let Some(d) = world.definition(&v.definition) else {
            return Ok(());
        };
        let role = &d.seats[usize::from(seat)];
        if v.destroyed || role.controls || role.weapon || d.family != Family::Wheeled {
            return Ok(());
        }
        let tick = self.simulation.state().tick;
        let free = |s: usize| {
            world.seat_occupant(v.id, s).is_none()
                && !self.bot_claimed(
                    Resource::Seat {
                        vehicle,
                        seat: s as u8,
                    },
                    bot,
                    tick,
                )
        };
        let target = d.control_seat().filter(|s| free(*s)).or_else(|| {
            d.weapon_seat().filter(|s| {
                free(*s)
                    && world.weapon_available(v.id)
                    && d.control_seat()
                        .and_then(|driver| world.seat_occupant(v.id, driver))
                        .is_some_and(|driver| self.bot_allies(bot, driver.owner.0))
            })
        });
        let Some(target) = target else { return Ok(()) };
        let count = d.seats.len();
        for _ in 0..count {
            self.switch_seat(bot, 1)?;
            if self.mounted(bot) == Some((vehicle, target as u8)) {
                break;
            }
        }
        Ok(())
    }

    pub(super) fn observe_bot_objects(&mut self) {
        self.bots.objects = self
            .vehicles
            .world
            .as_ref()
            .map_or_else(Vec::new, |w| w.snapshot(&self.simulation.physics).vehicles);
        self.bots.interaction_budget = 32;
        let tick = self.simulation.state().tick;
        self.bots.claims.prune(tick);
        for v in &self.bots.objects {
            if v.destroyed {
                self.bots
                    .claims
                    .release_resource(Resource::Body { vehicle: v.id.0 });
            }
            for s in &v.seats {
                let retained_objective_seat = s.occupant.is_some_and(|occupant| {
                    self.bots
                        .brains
                        .get(&occupant.owner.0)
                        .is_some_and(|brain| {
                            brain.objective.drive(self, occupant.owner.0)
                                == Some((v.id.0, s.index as u8))
                        })
                });
                if v.destroyed || s.occupant.is_some() && !retained_objective_seat {
                    self.bots.claims.release_resource(Resource::Seat {
                        vehicle: v.id.0,
                        seat: s.index as u8,
                    });
                }
            }
        }
        let invalid: Vec<_> = self
            .bots
            .brains
            .iter()
            .filter(|(owner, b)| {
                b.resting
                    || !self.is_alive(**owner)
                    || self.seated(**owner) && b.objective.drive(self, **owner).is_none()
            })
            .map(|(o, _)| *o)
            .collect();
        for o in invalid {
            self.bots.claims.release_owner(o);
        }
    }

    fn bot_claimed(&self, resource: Resource, except: OwnerId, tick: u64) -> bool {
        self.bots
            .claims
            .contending_claim(resource, tick, |o| {
                o == except || self.bot_allies(except, o)
            })
            .is_some_and(|c| c.owner != except)
    }

    /// Live claimants on `bot`'s side. Claims coordinate allies; opponents'
    /// intentions on the same loose body are a contest (`claims::contends`).
    pub(super) fn claim_allies(&self, bot: OwnerId, tick: u64) -> BTreeSet<OwnerId> {
        self.bots
            .claims
            .claimants(tick)
            .filter(|o| *o == bot || self.bot_allies(bot, *o))
            .collect()
    }

    fn bot_crew(&self, bot: OwnerId, v: &VehicleSnapshot, tick: u64) -> bool {
        v.seats.iter().any(|s| {
            self.vehicles
                .world
                .as_ref()
                .and_then(|w| w.seat_occupant(v.id, s.index))
                .is_some_and(|o| self.bot_allies(bot, o.owner.0))
        }) || self.bots.brains.keys().any(|other| {
            *other != bot
                && self.bot_allies(bot, *other)
                && self
                    .bots
                    .claims
                    .owner_claim(*other, tick)
                    .is_some_and(|c| c.resource.vehicle() == v.id.0)
        })
    }

    /// The same oriented hull-side approach for all ordinary boarding intents.
    pub(super) fn bot_seat_approach(
        &self,
        bot: OwnerId,
        v: &VehicleSnapshot,
        seat: u8,
    ) -> Option<Vec3> {
        let peer = self.peers.get(&bot)?;
        let feet = Vec3::from(peer.player.state().feet);
        let d = self.vehicles.world.as_ref()?.definition(&v.definition)?;
        let s = v.seats.get(usize::from(seat))?;
        let rotation = glam::Quat::from_array(v.transform.rotation);
        let local_seat = rotation.inverse()
            * (Vec3::from(s.transform.position) - Vec3::from(v.transform.position));
        let margin = peer.player.tuning().width * 0.5 + 0.2;
        let min = Vec3::from(d.bounds_min) * v.scale;
        let max = Vec3::from(d.bounds_max) * v.scale;
        let x = local_seat.x.clamp(min.x, max.x);
        let z = local_seat.z.clamp(min.z, max.z);
        let candidates = [
            Vec3::new(min.x - margin, 0.0, z),
            Vec3::new(max.x + margin, 0.0, z),
            Vec3::new(x, 0.0, min.z - margin),
            Vec3::new(x, 0.0, max.z + margin),
        ];
        candidates
            .into_iter()
            .map(|p| {
                let p = Vec3::from(v.transform.position) + rotation * p;
                Vec3::new(
                    p.x,
                    Vec3::from(v.transform.position).y
                        + d.wheels
                            .iter()
                            .map(|w| (w.position[1] - w.radius - w.rest_length) * v.scale)
                            .fold(d.bounds_min[1] * v.scale, f32::min),
                    p.z,
                )
            })
            .filter(|p| p.distance(Vec3::from(s.transform.position)) <= d.mount_distance * v.scale)
            .min_by(|a, b| {
                feet.distance_squared(*a)
                    .total_cmp(&feet.distance_squared(*b))
            })
    }

    /// A currently legal opportunity from observed object capabilities.
    /// Occupancy and authority are rechecked when the action is executed.
    fn bot_opportunity(
        &self,
        bot: OwnerId,
        v: &VehicleSnapshot,
        resource: Resource,
        (subject, enemy): (OwnerId, Vec3),
        idle: bool,
        tick: u64,
    ) -> Option<Opportunity> {
        let peer = self.peers.get(&bot)?;
        let feet = Vec3::from(peer.player.state().feet);
        let world = self.vehicles.world.as_ref()?;
        let d = world.definition(&v.definition)?;
        if v.destroyed
            || self.bot_claimed(resource, bot, tick)
            || feet.distance(Vec3::from(v.transform.position)) > DISCOVER
            || self.bots.claims.cooling_down(bot, resource, tick)
        {
            return None;
        }
        match resource {
            Resource::Seat { seat, .. } => {
                v.seats.get(usize::from(seat))?;
                let role = d.seats.get(usize::from(seat))?;
                if world.seat_occupant(v.id, usize::from(seat)).is_some()
                    || d.family != Family::Wheeled
                    || d.wheeled_flight.is_some()
                    || d.max_speed <= 0.0
                    || Vec3::from(v.velocity).length() > 2.0
                        && self.bots.claims.owner_claim(bot, tick).is_none()
                    || !self.can_ride(bot, v.owner.0)
                    || !self
                        .archetypes
                        .resolve(peer.player.state().archetype)
                        .can_ride
                    || v.seats.iter().any(|s| {
                        world
                            .seat_occupant(v.id, s.index)
                            .is_some_and(|o| !self.bot_allies(bot, o.owner.0))
                    })
                {
                    return None;
                }
                if role.weapon && !world.weapon_available(v.id) {
                    return None;
                }
                let crew = self.bot_crew(bot, v, tick);
                // Complete roles before filling passenger seats. An empty
                // vehicle needs someone at its controls before a passenger
                // has a reason to board it.
                let utility = if idle {
                    // Idle play: ride along in a teammate's vehicle.
                    let driven = d
                        .control_seat()
                        .and_then(|s| world.seat_occupant(v.id, s))
                        .is_some_and(|o| self.bot_allies(bot, o.owner.0));
                    if role.controls || role.weapon || !driven {
                        return None;
                    }
                    IDLE
                } else if role.controls {
                    0.84
                } else if crew && role.weapon && d.weapon.is_some() {
                    0.88
                } else if crew && feet.distance(enemy) > 12.0 {
                    0.72
                } else {
                    return None;
                };
                let point = self.bot_seat_approach(bot, v, seat)?;
                // A seat is a leg of the way to the enemy (`route`): an
                // armed vehicle serves the fight itself; any other only
                // when walking to it, boarding and driving there beats
                // walking. One that runs over an enemy on foot whom the
                // rules let this bot hurt (`can_damage_player`) is the
                // blow itself, so it is driven at them flat out, not
                // eased up to at cruise.
                let runs_over = d.runover_damage > 0.0
                    && self.mounted(subject).is_none()
                    && self.can_damage_player(bot, subject, false);
                let cruise = if runs_over {
                    d.max_speed
                } else {
                    d.max_speed * crate::route::CRUISE
                };
                // A blow is struck sooner from a seat nearer than the enemy.
                let strikes = runs_over && feet.distance(point) < feet.distance(enemy);
                if !idle
                    && d.weapon.is_none()
                    && !strikes
                    && !crate::route::drive_serves(
                        peer.player.tuning().forward,
                        feet.distance(enemy),
                        feet.distance(point),
                        Vec3::from(v.transform.position).distance(enemy),
                        cruise,
                    )
                {
                    return None;
                }
                Some(Opportunity {
                    resource,
                    point,
                    utility,
                })
            }
            Resource::Body { .. } => {
                if d.is_actor()
                    || !v.seats.is_empty()
                    || !(d.shove
                        || d.runover_damage > 0.0
                        || d.runover_push > 0.0
                        || d.smash.is_some())
                    || !self.may_move(bot, ObjectRef::Vehicle(v.id.0))
                    || self.object_held(ObjectRef::Vehicle(v.id.0))
                    // Idle play leaves a body anyone else means to work.
                    || idle
                        && self
                            .bots
                            .claims
                            .claimants_on(v.id.0, tick)
                            .any(|o| o != bot)
                {
                    return None;
                }
                let at = object_centre(v, d);
                let toward = flat(enemy - at);
                let short = if idle { IDLE_SHORT } else { 0.0 };
                if !(3.0 + short..=20.0).contains(&toward.length()) {
                    return None;
                }
                let direction = toward.normalize();
                let half = (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)) * (v.scale * 0.5);
                let inverse = glam::Quat::from_array(v.transform.rotation).inverse();
                // An oriented body's support extent follows the push axis;
                // using an unrotated max width puts approaches inside corners.
                let extent = |axis: Vec3| {
                    if d.family == Family::Ball {
                        half.max_element()
                    } else {
                        (inverse * axis).abs().dot(half)
                    }
                };
                let width = peer.player.tuning().width;
                let approach = push_approach(
                    feet,
                    at,
                    direction,
                    half.length() + width * 0.5 + 0.15,
                    width,
                )?;
                let radius = extent(Vec3::new(-direction.z, 0.0, direction.x));
                let point = Vec3::new(approach.point.x, feet.y, approach.point.z);
                if self.bot_ally_corridor(
                    bot,
                    at,
                    toward.normalize(),
                    toward.length() - short,
                    radius,
                ) {
                    return None;
                }
                // Mass affects expected acceleration, not permission or an invented force.
                // A heavy body remains useful on a slope, but costs more commitment.
                let effort = (d.mass / combat::PLAYER_MASS).sqrt().min(5.0) * 0.008;
                let base = if idle {
                    IDLE
                } else {
                    0.85
                };
                Some(Opportunity {
                    resource,
                    point,
                    utility: base - effort,
                })
            }
        }
    }

    /// `seats`: whether a seat may serve its goal at all (not while it has
    /// a grounded objective, whose own plan decides what it drives).
    pub(super) fn bot_interaction(
        &mut self,
        bot: OwnerId,
        enemy: Option<Knowledge>,
        seats: bool,
        tick: u64,
    ) -> Option<Opportunity> {
        let brain = &self.bots.brains[&bot];
        if self.seated(bot)
            || tick < brain.next_interaction
            || brain
                .kind
                .weight("interact")
                == 0.0
        {
            self.bots.claims.release_owner(bot);
            return None;
        }
        // With no enemy about, idle play toward a player in sight (`extras`).
        let (subject, toward, idle) = match enemy {
            Some(enemy) => (enemy.subject, enemy.at, false),
            None => match self.bot_idle_mark(bot, tick) {
                Some((mark, at)) => (mark, at, true),
                None => {
                    self.bots.claims.release_owner(bot);
                    return None;
                }
            },
        };
        let brain = &self.bots.brains[&bot];
        let feet = Vec3::from(self.peers[&bot].player.state().feet);
        if let Some(claim) = self.bots.claims.owner_claim(bot, tick) {
            let opportunity = (claim.subject == subject
                && (seats || matches!(claim.resource, Resource::Body { .. })))
            .then(|| {
                self.bots
                    .objects
                    .iter()
                    .find(|v| v.id.0 == claim.resource.vehicle())
                    .and_then(|v| {
                        self.bot_opportunity(bot, v, claim.resource, (subject, toward), idle, tick)
                    })
            })
            .flatten();
            if let Some(mut o) = opportunity {
                self.bots
                    .claims
                    .progress(bot, feet.distance(o.point), false, tick);
                o.utility -= feet.distance(o.point) * TRAVEL_COST;
                return Some(o);
            }
            self.bots.claims.fail(bot, claim.resource, tick);
        }
        // Bounded discovery with a rotating continuation. One sight query per
        // object, then cheap role evaluation; a crowded world cannot multiply
        // raycasts by seats * bots * objects in a frame.
        let len = self.bots.objects.len();
        if len == 0 {
            return None;
        }
        let start = self.bots.brains[&bot].object_cursor % len;
        let mut best: Option<Opportunity> = None;
        let mut visited = 0;
        for offset in 0..len.min(OBJECTS_PER_BOT) {
            if self.bots.interaction_budget == 0 {
                break;
            }
            visited += 1;
            let v = &self.bots.objects[(start + offset) % len];
            let at = Vec3::from(v.transform.position);
            if v.destroyed || feet.distance(at) > DISCOVER {
                continue;
            }
            self.bots.interaction_budget -= 1;
            let eye = self.peers[&bot].player.eye();
            if self
                .bot_sees(
                    bot,
                    Some(sightlines::Subject::Vehicle(v.id.0)),
                    eye,
                    at + Vec3::Y,
                    brain.kind.sight,
                    sightlines::Urgency::Ordinary,
                )
                .is_none()
            {
                continue;
            }
            for resource in v
                .seats
                .iter()
                .map(|s| Resource::Seat {
                    vehicle: v.id.0,
                    seat: s.index as u8,
                })
                .chain(std::iter::once(Resource::Body { vehicle: v.id.0 }))
                .filter(|r| seats || matches!(r, Resource::Body { .. }))
            {
                if let Some(mut o) =
                    self.bot_opportunity(bot, v, resource, (subject, toward), idle, tick)
                {
                    if brain.kind.weight("chase") == 0.0
                        && feet.distance(o.point) > 2.0
                    {
                        continue;
                    }
                    o.utility -= feet.distance(o.point) * TRAVEL_COST;
                    if best.is_none_or(|old| o.utility > old.utility) {
                        best = Some(o);
                    }
                }
            }
        }
        self.bots.brains.get_mut(&bot)?.object_cursor = (start + visited.max(1)) % len;
        let opportunity = best?;
        let allies = self.claim_allies(bot, tick);
        if self.bots.claims.acquire(
            bot,
            subject,
            opportunity.resource,
            feet.distance(opportunity.point),
            tick,
            |o| allies.contains(&o),
        ) {
            let brain = self.bots.brains.get_mut(&bot)?;
            brain.push_contact = None;
            brain.push_anchor = self
                .bots
                .objects
                .iter()
                .find(|v| v.id.0 == opportunity.resource.vehicle())
                .and_then(|v| {
                    self.vehicles
                        .world
                        .as_ref()?
                        .definition(&v.definition)
                        .map(|d| (v.id.0, object_centre(v, d)))
                });
            Some(opportunity)
        } else {
            None
        }
    }

    /// Intentional contact avoids allied bodies, whether or not friendly fire
    /// would allow damage. Occupied allied chassis also count as a corridor.
    fn bot_ally_corridor(
        &self,
        bot: OwnerId,
        at: Vec3,
        direction: Vec3,
        length: f32,
        radius: f32,
    ) -> bool {
        let within = |p: Vec3, extra: f32| {
            let delta = flat(p - at);
            let along = delta.dot(direction);
            along >= -extra
                && along <= length + extra
                && (delta - direction * along).length() < radius + extra
        };
        self.peers.iter().any(|(o, p)| {
            *o != bot
                && p.combat.alive
                && !self.seated(*o)
                && self.bot_allies(bot, *o)
                && within(
                    Vec3::from(p.player.state().feet),
                    p.player.tuning().width * 0.5,
                )
        }) || self.bots.objects.iter().any(|v| {
            self.mounted(bot).is_none_or(|(id, _)| id != v.id.0)
                && v.seats.iter().any(|s| {
                    s.occupant
                        .is_some_and(|o| o.owner.0 != bot && self.bot_allies(bot, o.owner.0))
                })
                && self
                    .vehicles
                    .world
                    .as_ref()
                    .and_then(|w| w.definition(&v.definition))
                    .is_some_and(|d| {
                        within(
                            Vec3::from(v.transform.position),
                            (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)).length()
                                * v.scale
                                * 0.5,
                        )
                    })
        })
    }

    /// Common ordinary boarding executor for combat opportunities and plans.
    /// Approaching is not success; native seat admission remains authoritative.
    pub(super) fn try_bot_board(
        &mut self,
        bot: OwnerId,
        vehicle: u64,
        seat: u8,
        tick: u64,
    ) -> Result<bool> {
        if self.mounted(bot) == Some((vehicle, seat)) {
            return Ok(true);
        }
        if !self
            .bots
            .objects
            .iter()
            .find(|v| v.id.0 == vehicle)
            .is_some_and(|v| Vec3::from(v.velocity).length() <= 2.0)
            || !self.vehicle_board_reach(bot, vehicle, seat)
        {
            return Ok(false);
        }
        match self.board_vehicle(bot, vehicle, seat) {
            Ok(()) => {
                let brain = self.bots.brains.get_mut(&bot).unwrap();
                brain.set_goal(None);
                brain.vehicle_headway.restart(tick);
                Ok(true)
            }
            Err(_) => {
                self.bots
                    .claims
                    .fail(bot, Resource::Seat { vehicle, seat }, tick);
                Ok(false)
            }
        }
    }

    pub(super) fn act_bot_interaction(
        &mut self,
        bot: OwnerId,
        enemy: Option<Vec3>,
        tick: u64,
    ) -> Result<Option<Vec3>> {
        let Some(claim) = self.bots.claims.owner_claim(bot, tick) else {
            return Ok(None);
        };
        match claim.resource {
            Resource::Seat { vehicle, seat } => {
                if self.try_bot_board(bot, vehicle, seat, tick)? {
                    self.bots.claims.release_owner(bot);
                }
                Ok(None)
            }
            Resource::Body { vehicle } => {
                // Idle play pushes toward the player it was aimed at.
                let idle = enemy.is_none();
                let Some(enemy) = enemy.or_else(|| {
                    self.bot_idle_mark(bot, tick)
                        .filter(|(mark, _)| *mark == claim.subject)
                        .map(|(_, at)| at)
                }) else {
                    return Ok(None);
                };
                let Some(v) = self.bots.objects.iter().find(|v| v.id.0 == vehicle) else {
                    return Ok(None);
                };
                let Some(o) = self.bot_opportunity(
                    bot,
                    v,
                    claim.resource,
                    (claim.subject, enemy),
                    idle,
                    tick,
                ) else {
                    return Ok(None);
                };
                let Some(d) = self
                    .vehicles
                    .world
                    .as_ref()
                    .and_then(|w| w.definition(&v.definition))
                else {
                    return Ok(None);
                };
                let at = object_centre(v, d);
                let feet = Vec3::from(self.peers[&bot].player.state().feet);
                let toward = flat(enemy - at).normalize_or_zero();
                let brain = self.bots.brains.get_mut(&bot).unwrap();
                if brain
                    .push_contact
                    .is_some_and(|(id, at)| id == vehicle && tick.saturating_sub(at) <= 4)
                    && brain.push_anchor.is_some_and(|(id, anchor)| {
                        id == vehicle && flat(at - anchor).dot(toward) >= 0.25
                    })
                {
                    self.bots
                        .claims
                        .progress(bot, feet.distance(o.point), true, tick);
                    brain.push_anchor = Some((vehicle, at));
                }
                let width = self.peers[&bot].player.tuning().width;
                let half = (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)) * (v.scale * 0.5);
                if push_approach(feet, at, toward, half.length() + width * 0.5 + 0.15, width)
                    .is_some_and(|approach| approach.pushing)
                {
                    Ok(Some(toward))
                } else {
                    Ok(None)
                }
            }
        }
    }

    pub(in crate::session) fn bot_push_progress(
        &mut self,
        owner: OwnerId,
        vehicle: u64,
        tick: u64,
    ) {
        if self
            .bots
            .claims
            .owner_claim(owner, tick)
            .is_some_and(|c| c.resource == (Resource::Body { vehicle }))
            && let Some(brain) = self.bots.brains.get_mut(&owner)
        {
            brain.push_contact = Some((vehicle, tick));
        }
    }

    /// The path describes fixed geometry. A small local sidestep lets living
    /// actors pass each other instead of hopping indefinitely at a shared
    /// waypoint; the ordinary motor still enforces physical clearance.
    /// Any ally in the way is passed. So is any other body, except
    /// `quarry`, the one it is going after, and one standing at `goal`, the
    /// spot it is making for, which it contests rather than walks round.
    /// It passes on its left, so two walking straight into each other both
    /// step aside the same way and get by, unless the one in the way
    /// already stands off to its left: that one is passed on the right,
    /// rather than crossed in front of. In a gap (solid close on both
    /// sides), an ally ahead already going its
    /// way is followed at its pace instead (`team` overlap: the later of
    /// two on one path gives way), so a file through a narrow gap keeps
    /// moving instead of every one stepping into the frame.
    pub(super) fn bot_walk_direction(
        &self,
        bot: OwnerId,
        desired: Vec3,
        quarry: Option<OwnerId>,
        goal: Option<Vec3>,
    ) -> Vec3 {
        if desired == Vec3::ZERO || self.seated(bot) {
            return desired;
        }
        let own = &self.peers[&bot].player;
        let feet = Vec3::from(own.state().feet);
        let ahead = |p: &super::Peer| {
            let at = Vec3::from(p.player.state().feet);
            let delta = flat(at - feet);
            let along = delta.dot(desired);
            let width = (own.tuning().width + p.player.tuning().width) * 0.5;
            along > 0.0 && along < width + 0.6 && (delta - desired * along).length() < width
        };
        let side = Vec3::new(-desired.z, 0.0, desired.x);
        let waist = feet + Vec3::Y * own.tuning().stand_height * 0.5;
        // A gap: something solid close on both sides.
        let no_room = [side, -side].iter().all(|d| {
            self.world_ray(waist, *d, own.tuning().width + 0.6)
                .is_some()
        });
        // The pace of an ally ahead going the same way, as a share of its own.
        let follow = self
            .peers
            .iter()
            .filter(|(o, p)| {
                no_room
                    && **o != bot
                    && p.combat.alive
                    && !self.seated(**o)
                    && self.bot_allies(bot, **o)
                    && ahead(p)
            })
            .map(|(_, p)| flat(Vec3::from(p.player.state().velocity)).dot(desired))
            .filter(|pace| *pace > 0.5)
            .map(|pace| (pace / own.tuning().forward.max(0.1)).min(1.0))
            .reduce(f32::min);
        if let Some(pace) = follow {
            return desired * pace;
        }
        // The nearest body in the way, and how far it stands off to the
        // left of the line (negative: to the right).
        let blocker = self
            .peers
            .iter()
            .filter(|(o, p)| {
                if **o == bot || !p.combat.alive || self.seated(**o) {
                    return false;
                }
                let at = Vec3::from(p.player.state().feet);
                if !self.bot_allies(bot, **o)
                    && (Some(**o) == quarry
                        || goal.is_some_and(|g| flat(at - g).length() < CONTESTED))
                {
                    return false;
                }
                ahead(p)
            })
            .map(|(_, p)| flat(Vec3::from(p.player.state().feet) - feet))
            .min_by(|a, b| a.dot(desired).total_cmp(&b.dot(desired)));
        match blocker {
            // One already off to its left is passed on its right, not
            // walked across; one dead ahead on its left.
            Some(delta) if delta.dot(side) > own.tuning().width * 0.1 => {
                (desired * 0.25 - side).normalize_or_zero()
            }
            Some(_) => (desired * 0.25 + side).normalize_or_zero(),
            None => desired,
        }
    }

    /// Whether walking `toward` takes the bot off an edge with no floor
    /// within a fall that hurts (its type's least hurting impact): no floor
    /// under its whole footprint a step or two ahead.
    /// The deepest fall that does not hurt the bot's body: from its player
    /// type's fall-damage data (the slowest impact that hurts) and gravity.
    fn bot_safe_drop(&self, bot: OwnerId) -> Option<f32> {
        let peer = self.peers.get(&bot)?;
        let state = peer.player.state();
        let gravity = peer.player.tuning().gravity.max(1.0);
        let impact = crate::player_types::PlayerType::from_archetype(state.archetype)
            .unwrap_or_default()
            .min_impact_speed()
            * state.scale.max(1.0);
        Some(impact * impact / (2.0 * gravity))
    }
    pub(super) fn bot_fall_ahead(
        &self,
        bot: OwnerId,
        feet: Vec3,
        body: &crate::nav::Body,
        toward: Vec3,
        off_route: bool,
    ) -> bool {
        let Some(hurts) = self.bot_safe_drop(bot) else {
            return false;
        };
        // Off its route (footwork, a strafe, a goof's walk) it keeps to
        // floor it can walk back up from; a route plans its own drops, so
        // on it no drop the route may take is refused, nor one that does
        // not hurt.
        let depth = if off_route {
            body.step + body.drop
        } else {
            hurts.max(body.drop + ROUTE_DROP_SLACK)
        };
        let across = Vec3::new(-toward.z, 0.0, toward.x) * (body.width * 0.4);
        [0.6, 1.2].into_iter().any(|ahead| {
            [Vec3::ZERO, across, -across].into_iter().all(|side| {
                let at = feet + toward * ahead + side + Vec3::Y * 0.5;
                self.world_ray(at, Vec3::NEG_Y, 0.5 + depth).is_none()
            })
        })
    }

    /// Where a bot standing on a body (a vehicle's roof, another player's
    /// head) with its enemy close below steps down to: the nearest world
    /// floor round it, no higher than its feet, open to walk to (no wall on
    /// the way, no player standing there), the side toward `toward` first
    /// among equally near ones. None when every side is closed.
    pub(super) fn bot_step_off(
        &self,
        bot: OwnerId,
        feet: Vec3,
        body: &crate::nav::Body,
        toward: Vec3,
        no_higher: f32,
    ) -> Option<Vec3> {
        const SIDES: usize = 16;
        let want = flat(toward - feet).normalize_or_zero();
        let depth = (feet.y - toward.y).max(0.0) + body.step + 0.5;
        let others: Vec<(Vec3, f32)> = self
            .peers
            .iter()
            .filter(|(o, p)| **o != bot && p.combat.alive && !self.seated(**o))
            .map(|(_, p)| (Vec3::from(p.player.state().feet), p.player.tuning().width))
            .collect();
        // Floor under a body (the vehicle it stands on, a crate beside it)
        // is no floor to stand on: the world ray sees through bodies.
        let world = self.vehicles.world.as_ref();
        let covered = |at: Vec3| {
            self.bots.objects.iter().any(|v| {
                let Some(d) = world.and_then(|w| w.definition(&v.definition)) else {
                    return false;
                };
                let rotation = glam::Quat::from_array(v.transform.rotation);
                let half = (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)) * v.scale * 0.5;
                let l = rotation.inverse() * (at - object_centre(v, d));
                let by = body.width * 0.5;
                !v.destroyed && l.x.abs() <= half.x + by && l.z.abs() <= half.z + by
            })
        };
        let mut best: Option<(f32, Vec3)> = None;
        for i in 0..SIDES {
            let angle = i as f32 * std::f32::consts::TAU / SIDES as f32;
            let d = Vec3::new(angle.cos(), 0.0, angle.sin());
            // Out in half-width steps to past a jeep's half length.
            for k in 1..=8 {
                let out = body.width * 0.5 * (k as f32 + 1.0);
                let at = feet + d * out;
                if self.world_ray(feet + Vec3::Y * body.step, d, out).is_some() {
                    break;
                }
                let Some(down) = self.world_ray(at + Vec3::Y * 0.5, Vec3::NEG_Y, 0.5 + depth)
                else {
                    continue;
                };
                let floor = at + Vec3::Y * (0.5 - down);
                if floor.y > no_higher {
                    continue;
                }
                let crowded = others.iter().any(|(p, width)| {
                    flat(*p - floor).length() < (body.width + width) * 0.5
                        && (p.y - floor.y).abs() < body.height
                });
                if crowded || covered(floor) {
                    continue;
                }
                let score = out - want.dot(d) * body.width * 0.5;
                if best.is_none_or(|(b, _)| score < b) {
                    best = Some((score, floor));
                }
                break;
            }
        }
        best.map(|(_, at)| at)
    }

    /// A vehicle is a body the walk grid leaves out (`nav`): one a walking
    /// bot is about to walk into is walked round by its nearer side, not
    /// pressed against (which pushes it, `movables`) or hopped onto. Not the
    /// one it stands on or is inside, makes for (`goal`, a seat beside it)
    /// or whose crew is its `quarry`.
    pub(super) fn bot_vehicle_detour(
        &self,
        bot: OwnerId,
        desired: Vec3,
        quarry: Option<OwnerId>,
        goal: Option<Vec3>,
    ) -> Vec3 {
        if desired == Vec3::ZERO || self.seated(bot) {
            return desired;
        }
        let own = &self.peers[&bot].player;
        let feet = Vec3::from(own.state().feet);
        let Some(world) = self.vehicles.world.as_ref() else {
            return desired;
        };
        let clearance = own.tuning().width * 0.5 + 0.1;
        for v in &self.bots.objects {
            let Some(d) = world.definition(&v.definition) else {
                continue;
            };
            let rotation = glam::Quat::from_array(v.transform.rotation);
            let centre = object_centre(v, d);
            let half = (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)) * v.scale * 0.5;
            let local = |at: Vec3| rotation.inverse() * (at - centre);
            let inside = |at: Vec3, by: f32| {
                let l = local(at);
                l.x.abs() <= half.x + by && l.z.abs() <= half.z + by
            };
            let on_top = local(feet).y > half.y - 0.3;
            let crewed_by_quarry = quarry.is_some_and(|q| {
                self.mounted(q)
                    .is_some_and(|(vehicle, _)| vehicle == v.id.0)
            });
            // A loose body with no seats (a ball, a crate) is something to
            // push or use, not a vehicle to keep clear of.
            if v.destroyed
                || d.seats.is_empty()
                || on_top
                || crewed_by_quarry
                // Its goal on it, or right beside it and within reach.
                || goal.is_some_and(|g| {
                    inside(g, 0.0)
                        || inside(g, clearance + 0.5) && flat(g - feet).length() < clearance * 2.0 + 1.0
                })
                || inside(feet, 0.0)
            {
                continue;
            }
            // Within a stride of walking into it.
            let ahead = (1..=4).any(|i| inside(feet + desired * (i as f32 * 0.25), clearance));
            if !ahead {
                continue;
            }
            let side = desired.cross(centre - feet).y;
            let tangent = if side > 0.0 {
                Vec3::new(desired.z, 0.0, -desired.x)
            } else {
                Vec3::new(-desired.z, 0.0, desired.x)
            };
            return (desired * 0.25 + tangent).normalize_or_zero();
        }
        desired
    }

    /// Local execution uses the actual oriented chassis, above its wheel
    /// contact plane. Static navigation is only a corridor proposal.
    fn bot_vehicle_clear(
        &self,
        bot: OwnerId,
        v: &VehicleSnapshot,
        direction: Vec3,
        reach: f32,
    ) -> bool {
        let Some(d) = self
            .vehicles
            .world
            .as_ref()
            .and_then(|w| w.definition(&v.definition))
        else {
            return false;
        };
        let min = Vec3::from(d.bounds_min) * v.scale;
        let max = Vec3::from(d.bounds_max) * v.scale;
        let mut half = (max - min) * 0.5;
        half.y = (half.y - 0.15).max(0.05);
        let local_centre = (max + min) * 0.5 + Vec3::Y * 0.15;
        let rotation = glam::Quat::from_array(v.transform.rotation);
        let at = Vec3::from(v.transform.position) + rotation * local_centre;
        let pose = Pose {
            translation: at,
            rotation,
            ..Default::default()
        };
        let shape = Cuboid::new(half);
        let own_tag = super::super::vehicles::VEHICLE_TAG | u128::from(v.id.0);
        let subject = self.bots.brains[&bot].memory.map(|k| k.subject);
        let predicate = |_: ColliderHandle, c: &Collider| {
            if c.user_data == own_tag {
                return false;
            }
            if c.user_data >> 64 == 1 {
                let owner = c.user_data as u64;
                if owner == bot || self.mounted(owner).is_some_and(|(id, _)| id == v.id.0) {
                    return false;
                }
                if subject == Some(owner)
                    && self.bot_enemy(bot, &self.bots.brains[&bot].kind, owner)
                {
                    return false;
                }
            }
            true
        };
        self.simulation
            .physics
            .query_pipeline_with_filter(
                QueryFilter::default()
                    .exclude_sensors()
                    .predicate(&predicate),
            )
            .cast_shape(
                &pose,
                direction * reach,
                &shape,
                ShapeCastOptions {
                    max_time_of_impact: 1.0,
                    stop_at_penetration: true,
                    ..Default::default()
                },
            )
            .is_none()
    }

    pub(super) fn bot_crew_ready(&self, bot: OwnerId, tick: u64) -> bool {
        let Some((vehicle, _)) = self.mounted(bot) else {
            return true;
        };
        let since = self.bots.brains[&bot]
            .vehicle_since
            .map_or(tick, |(_, since)| since);
        if tick >= since + CREW_WAIT {
            return true;
        }
        !self.bots.brains.keys().any(|other| {
            *other != bot
                && self.bot_allies(bot, *other)
                && self.bots.claims.owner_claim(*other, tick).is_some_and(|c| {
                    c.resource.vehicle() == vehicle
                        && match c.resource {
                            Resource::Seat { seat, .. } => self
                                .vehicles
                                .world
                                .as_ref()
                                .and_then(|w| w.definition_of(VehicleId(vehicle)))
                                .is_some_and(|d| d.control_seat() == Some(usize::from(seat))),
                            _ => false,
                        }
                })
        })
    }

    pub(super) fn bot_weapon_origin(&self, bot: OwnerId) -> Option<Vec3> {
        let (id, seat) = self.mounted(bot)?;
        let w = self.vehicles.world.as_ref()?;
        let v = self.bots.objects.iter().find(|v| v.id.0 == id)?;
        let d = w.definition(&v.definition)?;
        if d.weapon_seat() != Some(usize::from(seat)) || !w.weapon_available(v.id) {
            return None;
        }
        let (local, _) = d.muzzle(v.turret_aim)?;
        Some(
            Vec3::from(v.transform.position)
                + glam::Quat::from_array(v.transform.rotation) * local * v.scale,
        )
    }

    /// No ally stands in the line of fire from `origin` to `target`, nor
    /// within `past` beyond the target, where a miss carries on. The line
    /// widens by `spread` radians, how far off its aim may send the shot.
    pub(super) fn bot_fire_clear(
        &self,
        bot: OwnerId,
        origin: Vec3,
        target: Vec3,
        splash: f32,
        past: f32,
        spread: f32,
    ) -> bool {
        let Some(space) = shot_space(origin, target, splash, past, spread) else {
            return false;
        };
        let mount = self.mounted(bot).map(|(v, _)| v);
        // The cheap geometry first: the side is looked up only for a body
        // in the way.
        !self.peers.iter().any(|(o, p)| {
            let half = p.player.tuning().stand_height * 0.5;
            *o != bot
                && p.combat.alive
                && space.holds(Vec3::from(p.player.state().feet) + Vec3::Y * half, half)
                && !(mount.is_some() && mount == self.mounted(*o).map(|(v, _)| v))
                && self.bot_allies(bot, *o)
        })
    }

    pub(super) fn bot_vehicle_body(&self, bot: OwnerId) -> Option<(Vec3, Body)> {
        let (vehicle, seat) = self.mounted(bot)?;
        let w = self.vehicles.world.as_ref()?;
        let d = w.definition_of(VehicleId(vehicle))?;
        if !d.seats.get(usize::from(seat))?.controls || d.family != Family::Wheeled {
            return None;
        }
        let v = w.vehicle_snapshot(&self.simulation.physics, VehicleId(vehicle))?;
        let min = Vec3::from(d.bounds_min) * v.scale;
        let support = d
            .wheels
            .iter()
            .map(|w| (w.position[1] - w.radius - w.rest_length) * v.scale)
            .fold(min.y, f32::min);
        let centre = (Vec3::from(d.bounds_min) + Vec3::from(d.bounds_max)) * v.scale * 0.5;
        let size = (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)) * v.scale;
        Some((
            Vec3::from(v.transform.position)
                + glam::Quat::from_array(v.transform.rotation)
                    * Vec3::new(centre.x, support, centre.z),
            Body {
                width: Vec3::new(size.x, 0.0, size.z).length(),
                height: Vec3::from(d.bounds_max).y * v.scale - support,
                crouch_height: Vec3::from(d.bounds_max).y * v.scale - support,
                step: 0.2,
                jump: 0.2,
                drop: 0.4,
                floor_cos: 0.85,
                conservative: true,
                bottom: (min.y - support).max(0.0),
                swims: false,
            },
        ))
    }

    /// A seat is a control adapter, not another brain. A driver follows the
    /// selected path; a gunner keeps the shared world aim; passengers turn
    /// relative to their seat. Everyone submits normal movement.
    pub(super) fn bot_seated_input(
        &mut self,
        bot: OwnerId,
        mut input: MoveInput,
        wanted: Option<Waypoint>,
        behaviour: Behaviour,
        tick: u64,
    ) -> Result<MoveInput> {
        let Some((vehicle, seat)) = self.mounted(bot) else {
            return Ok(input);
        };
        let w = self.vehicles.world.as_ref().context("No vehicle world")?;
        let v = w
            .vehicle_snapshot(&self.simulation.physics, VehicleId(vehicle))
            .context("No vehicle")?;
        let d = w
            .definition(&v.definition)
            .context("No vehicle definition")?;
        // An affordance lost: a wreck, or a wheeled hull on its side or its
        // roof, ends the drive (and any ride in it) at once.
        if v.destroyed
            || d.family == Family::Wheeled
                && d.wheeled_flight.is_none()
                && !crate::route::upright(v.transform.rotation)
        {
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.next_interaction = tick + GIVE_UP_TICKS;
            brain.plan.clear();
            brain.search = None;
            let _ = self.dismount_vehicle(bot);
            return Ok(MoveInput {
                yaw: self.bots.brains[&bot].yaw,
                ..Default::default()
            });
        }
        let role = &d.seats[usize::from(seat)];
        // A seat with neither controls nor a weapon carries a bot, but it
        // fights nothing from there.
        // A passenger holding a weapon of its own that reaches from there
        // (a gun, not a sword) fights from its seat.
        let armed = self.weapons.actor(ActorId(bot)).is_some_and(|a| {
            let scale = self.peers.get(&bot).map_or(1.0, |p| p.player.state().scale);
            a.inventory
                .iter()
                .flatten()
                .any(|i| hand_combat::item_attacks_from_afar(self, i, scale))
        });
        let carried_only = !role.controls && !role.weapon && !armed;
        // Idle play: a passenger rides along while a teammate drives.
        let riding_along = !role.controls
            && !role.weapon
            && d.control_seat()
                .and_then(|s| w.seat_occupant(v.id, s))
                .is_some_and(|o| self.bot_allies(bot, o.owner.0));
        // The bot knows its seat immediately. Human clients report the same
        // handover through SeatSince when they learn where they are sitting.
        let peer = self.peers.get_mut(&bot).unwrap();
        if !peer
            .seat_since
            .is_some_and(|(_, old)| old.is_some_and(|s| s.vehicle == vehicle && s.seat == seat))
        {
            peer.seat_since = Some((
                tick,
                Some(SeatSince {
                    vehicle,
                    seat,
                    since: self.bots.brains[&bot].sequence,
                }),
            ));
        }
        input.jet = false;
        input.crouch = false;
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        let since = match brain.vehicle_since {
            Some((id, since)) if id == vehicle => since,
            _ => {
                brain.vehicle_since = Some((vehicle, tick));
                brain.vehicle_headway.clear();
                tick
            }
        };
        if role.controls && d.family == Family::Wheeled {
            let hull = super::super::vehicles::heading(v.transform.rotation);
            let at = Vec3::from(v.transform.position);
            let speed = Vec3::from(v.velocity).length();
            let lookahead = (1.5 + speed * 0.35).min(8.0);
            let mut next = wanted;
            for p in self.bots.brains[&bot].plan.iter().take(LOOKAHEAD_POINTS) {
                let delta = flat(p.feet - at);
                if p.through.is_some() || delta.length() > lookahead {
                    break;
                }
                if !self.bot_vehicle_clear(bot, &v, delta.normalize_or_zero(), delta.length()) {
                    break;
                }
                next = Some(*p);
            }
            // A chassis that runs over the enemy it fights on foot is the
            // blow itself: with no route left to them (they are too close
            // for one), it is driven at them, backing out first when they
            // are inside its turning circle (`route::gear`).
            let strike = self.bots.brains[&bot]
                .target
                .filter(|t| {
                    matches!(behaviour, Behaviour::Fight | Behaviour::Chase)
                        && d.runover_damage > 0.0
                        && self.mounted(*t).is_none()
                        && self.can_damage_player(bot, *t, false)
                })
                .and_then(|t| self.peers.get(&t))
                .filter(|p| p.combat.alive)
                .map(|p| flat(Vec3::from(p.player.state().feet) - at));
            let toward = next
                .filter(|p| p.through.is_none())
                .map(|p| flat(p.feet - at))
                .or(strike);
            let error = toward.map_or(0.0, |d| wrap(yaw_to(d) - hull));
            // Pure pursuit that knows how tightly this chassis turns: a
            // point inside its turning circle is backed out of, never
            // circled (`route::gear`). A chassis that made no headway backs
            // straight up for a while.
            let chassis = crate::route::Chassis::of(
                d.wheels
                    .iter()
                    .map(|w| (w.position[2] * v.scale, w.steering)),
                d.max_steering,
                (d.bounds_max[2] - d.bounds_min[2]) * v.scale,
            );
            let yaw_rate = -v.angular_velocity[1];
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            let turn = chassis.radius();
            let cruise = (
                d.max_speed * crate::route::CRUISE,
                d.reverse_speed * crate::route::CRUISE,
            );
            // One reversing rule (`route::gear`): the kind's policy says
            // what counts as behind, and how far back a pursued target is
            // backed onto rather than turned round for.
            let pursuing = matches!(
                behaviour,
                Behaviour::Fight | Behaviour::Chase | Behaviour::Search
            );
            let drive = crate::route::Driving {
                radius: turn,
                // It arrives once its side passes the point.
                reach: (d.bounds_max[0] - d.bounds_min[0]) * v.scale * 0.5,
                cruise,
                behind: REVERSE_DEGREES.to_radians(),
                reverse_limit: if pursuing {
                    REVERSE_DISTANCE
                } else {
                    f32::INFINITY
                },
            };
            let gear = match toward {
                // As of the tick before: this tick's headway is noted below.
                _ if backing_up(brain.vehicle_headway.idle(tick).saturating_sub(1)) => {
                    crate::route::Gear::Reverse { nose: false }
                }
                Some(delta) => crate::route::gear(&drive, error, delta.length(), brain.drive_gear),
                None => crate::route::Gear::Forward,
            };
            brain.drive_gear = gear;
            let (travel_sign, heading_error) = gear.steer(error);
            let hull_forward = flat(glam::Quat::from_array(v.transform.rotation) * Vec3::NEG_Z)
                .normalize_or_zero();
            let signed_speed = flat(Vec3::from(v.velocity)).dot(hull_forward);
            // Reversing changes both the desired hull heading and the tyres'
            // yaw response. During a direction change, brake first and steer
            // according to the direction the chassis is actually rolling.
            let response_sign = if signed_speed.abs() > 0.5 {
                signed_speed.signum()
            } else {
                travel_sign
            };
            let desired_steer = (response_sign * (heading_error * 1.5 - yaw_rate * 0.25))
                .clamp(-d.max_steering, d.max_steering);
            let previous = self.peers[&bot].input.yaw;
            // Mouse steering accumulates turn. Close its loop against the
            // actual steering angle rather than feeding enemy aim into it.
            input.yaw = wrap(previous + (desired_steer - v.mouse_steering[0]).clamp(-0.04, 0.04));
            input.pitch = 0.0;
            input.right = 0.0;
            let crew_waiting = self.bots.brains.keys().any(|other| {
                *other != bot
                    && self.bot_allies(bot, *other)
                    && self
                        .bots
                        .claims
                        .owner_claim(*other, tick)
                        .is_some_and(|c| c.resource.vehicle() == vehicle)
            });
            let braking = (d.brake_force / d.mass).max(1.0);
            let stopping = speed * 0.3 + speed * speed / (2.0 * braking) + 1.0;
            let travel = hull_forward * travel_sign;
            let radius = (d.bounds_max[0] - d.bounds_min[0]) * v.scale * 0.5;
            let hazard = self.bot_ally_corridor(bot, at, travel, stopping + 2.0, radius)
                || !self.bot_vehicle_clear(bot, &v, travel, stopping);
            let waiting = crew_waiting && tick < since + CREW_WAIT && speed < 2.0;
            let distance = toward.map_or(0.0, |delta| delta.length());
            // The leave leg (`route`): a chassis that cannot hurt the one it
            // chases (no gun, and no runover for someone not on foot) only
            // carries the bot there. It pulls up with its nose about as far
            // off as the bot fights from on foot (the standoff), never
            // ramming them; once there, and on foot it would still close on
            // them (they draw away slower than it walks), it gets out; one
            // outrunning a walker is followed in the seat.
            let walk = self.peers[&bot].player.tuning().forward;
            let target = self.bots.brains[&bot].target.and_then(|t| {
                self.peers.get(&t).map(|p| {
                    let state = p.player.state();
                    (
                        self.mounted(t).is_some(),
                        Vec3::from(state.feet),
                        Vec3::from(state.velocity),
                    )
                })
            });
            let standoff = (d.bounds_max[2] - d.bounds_min[2]) * v.scale * 0.5 + LEAVE_REACH;
            let carried = target.filter(|(mounted, _, _)| {
                matches!(behaviour, Behaviour::Chase | Behaviour::Fight)
                    && d.weapon.is_none()
                    && (*mounted || d.runover_damage <= 0.0)
            });
            let gap = carried.map(|(_, enemy, _)| flat(enemy - at).length() - standoff);
            let corner_speed =
                crate::route::pace(turn, gear, heading_error, distance, cruise).max(2.0);
            let arrival_speed =
                (2.0 * braking * gap.map_or(distance, |g| distance.min(g)).max(0.0))
                    .sqrt()
                    .max(2.0);
            let brake = signed_speed * travel_sign < -0.5
                || speed > corner_speed.min(arrival_speed)
                || gap.is_some_and(|g| g <= 0.0) && travel_sign > 0.0;
            input.forward = if toward.is_none() || hazard || waiting || brake {
                0.0
            } else {
                travel_sign * (1.0 - heading_error.abs() * 0.3).clamp(0.25, 0.8)
            };
            input.jump = input.forward == 0.0;
            let leave = carried.is_some_and(|(_, enemy, velocity)| {
                let toward = flat(enemy - at);
                toward.length() <= standoff && crate::route::walk_closes(walk, toward, velocity)
            });
            if leave {
                input.forward = 0.0;
                input.jump = true;
            }
            // Crew claims are bounded independently of steering. A stuck
            // chassis also gives up instead of permanently holding a seat.
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            let pursuing = brain.memory.is_some()
                || matches!(behaviour, Behaviour::Return | Behaviour::Objective);
            // Headway is getting somewhere: a chassis rocking back and
            // forth against what blocks it keeps moving but goes nowhere.
            // Braking for an impassable route is also no headway; throttle
            // and contact impulses alone cannot renew a task.
            if waiting || !pursuing {
                brain.vehicle_headway.mark(at, tick);
            } else {
                brain.vehicle_headway.note(at, tick, |mark, at| {
                    flat(at - mark).length() >= VEHICLE_PROGRESS
                });
            }
            let stuck = brain.vehicle_headway.idle(tick);
            if backing_up(stuck) && !hazard {
                input.forward = -0.5;
                input.jump = false;
            }
            if stuck >= VEHICLE_STALLED && stuck.is_multiple_of(120) {
                brain.plan.clear();
                brain.search = None;
                brain.settled = false;
            }
            if stuck >= VEHICLE_GIVE_UP {
                brain.next_interaction = tick + GIVE_UP_TICKS;
                brain.vehicle_headway.restart(tick);
                let _ = self.dismount_vehicle(bot);
                input = MoveInput {
                    yaw: self.bots.brains[&bot].yaw,
                    ..Default::default()
                };
            }
            if leave && speed < 3.0 {
                let brain = self.bots.brains.get_mut(&bot).unwrap();
                brain.next_interaction = tick + GIVE_UP_TICKS;
                brain.plan.clear();
                brain.search = None;
                let _ = self.dismount_vehicle(bot);
                return Ok(MoveInput {
                    yaw: self.bots.brains[&bot].yaw,
                    ..Default::default()
                });
            }
        } else {
            let has_driver = d
                .control_seat()
                .is_some_and(|s| w.seat_occupant(v.id, s).is_some());
            let needs_travel = matches!(
                behaviour,
                Behaviour::Chase | Behaviour::Search | Behaviour::Return | Behaviour::Objective
            );
            let unusable_gun = role.weapon && !w.weapon_available(v.id);
            input.forward = 0.0;
            input.right = 0.0;
            input.jump = false;
            if unusable_gun || needs_travel && !has_driver && tick >= since + CREW_WAIT {
                self.bots.brains.get_mut(&bot).unwrap().next_interaction = tick + GIVE_UP_TICKS;
                let _ = self.dismount_vehicle(bot);
                return Ok(MoveInput {
                    yaw: self.bots.brains[&bot].yaw,
                    ..Default::default()
                });
            }
            if !role.weapon {
                input.yaw = wrap(
                    input.yaw
                        - super::super::vehicles::heading(
                            v.seats[usize::from(seat)].transform.rotation,
                        ),
                );
            }
        }
        // Once no enemy or remembered task remains, safely relinquish the
        // vehicle. A stationary gunner without a driver need not sit forever.
        if behaviour == Behaviour::Wander
            && super::cadence::beat(bot, super::cadence::salt::DISMOUNT, tick, CREW_WAIT)
            && !riding_along
            // Carried to an enemy now in its band: it gets off to fight.
            || carried_only && behaviour == Behaviour::Fight
        {
            if carried_only && behaviour == Behaviour::Fight {
                // Not straight back on board for the fight it got off for.
                self.bots.brains.get_mut(&bot).unwrap().next_interaction = tick + GIVE_UP_TICKS;
            }
            let _ = self.dismount_vehicle(bot);
        }
        Ok(input)
    }
}

/// How much deeper than a body's planned drop (`nav::Body::drop`) the edge
/// check still finds floor on its route: the route planner samples floors
/// at cell centres, so a drop it plans can measure a little deeper from
/// where the body stands at the edge.
const ROUTE_DROP_SLACK: f32 = 0.5;
/// How far a driven chassis must get from where it last made headway for
/// that to count as headway again.
const VEHICLE_PROGRESS: f32 = 3.0;
/// Ticks without headway after which a driven chassis backs straight up for
/// a second, and plans again every second.
const VEHICLE_STALLED: u64 = 360;
fn backing_up(stuck: u64) -> bool {
    stuck > VEHICLE_STALLED && stuck < VEHICLE_STALLED + 120
}
/// Ticks without headway after which the driver gets out.
const VEHICLE_GIVE_UP: u64 = 720;
