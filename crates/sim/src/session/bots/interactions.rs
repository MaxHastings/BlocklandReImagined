//! Environmental opportunities and short, exclusive commitments. The brain
//! still chooses, paths and aims; this module discovers object capabilities
//! and translates the chosen action into ordinary player controls.
use super::*;
use bri_vehicles::{Family, VehicleId, VehicleSnapshot};
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;

const DISCOVER: f32 = 24.0;
const RETRY: u64 = 240;
const CREW_WAIT: u64 = 360;
const OBJECTS_PER_BOT: usize = 8;
const LOOKAHEAD_POINTS: usize = 24;
/// How far past its own half-length a driver that only carries its bot to
/// an enemy stops and gets out: about where the bot fights from on foot.
const LEAVE_REACH: f32 = 3.0;

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
#[derive(Clone, Copy)]
pub(super) struct Opportunity {
    resource: Resource,
    pub point: Vec3,
    pub utility: f32,
}

impl Session {
    /// A passenger may have boarded the reachable lower seat of a tall vehicle.
    /// Fill its useful empty role through the same seat keys players use.
    pub(super) fn promote_bot_seat(&mut self, bot: OwnerId) -> Result<()> {
        if !self.is_alive(bot)
            || self.bots.brains.get(&bot).is_none_or(|b| {
                b.resting || b.kind.behaviours.get("interact").copied().unwrap_or(0.0) <= 0.0
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
            .resource_claim(resource, tick)
            .is_some_and(|c| c.owner != except)
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
        enemy: Vec3,
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
                let utility = if role.controls {
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
                // walking.
                if d.weapon.is_none()
                    && !crate::route::drive_serves(
                        peer.player.tuning().forward,
                        feet.distance(enemy),
                        feet.distance(point),
                        Vec3::from(v.transform.position).distance(enemy),
                        d.max_speed * crate::route::CRUISE,
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
                {
                    return None;
                }
                let at = object_centre(v, d);
                let toward = flat(enemy - at);
                if !(3.0..=20.0).contains(&toward.length()) {
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
                if self.bot_ally_corridor(bot, at, toward.normalize(), toward.length(), radius) {
                    return None;
                }
                // Mass affects expected acceleration, not permission or an invented force.
                // A heavy body remains useful on a slope, but costs more commitment.
                let effort = (d.mass / combat::PLAYER_MASS).sqrt().min(5.0) * 0.008;
                Some(Opportunity {
                    resource,
                    point,
                    utility: 0.85 - effort,
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
                .behaviours
                .get("interact")
                .copied()
                .unwrap_or(0.0)
                == 0.0
            || enemy.is_none()
        {
            self.bots.claims.release_owner(bot);
            return None;
        }
        let enemy = enemy?;
        let feet = Vec3::from(self.peers[&bot].player.state().feet);
        if let Some(claim) = self.bots.claims.owner_claim(bot, tick) {
            let opportunity = (claim.subject == enemy.subject
                && (seats || matches!(claim.resource, Resource::Body { .. })))
            .then(|| {
                self.bots
                    .objects
                    .iter()
                    .find(|v| v.id.0 == claim.resource.vehicle())
                    .and_then(|v| self.bot_opportunity(bot, v, claim.resource, enemy.at, tick))
            })
            .flatten();
            if let Some(mut o) = opportunity {
                self.bots
                    .claims
                    .progress(bot, feet.distance(o.point), false, tick);
                o.utility -= feet.distance(o.point) * 0.002;
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
            if self
                .simulation
                .sight(
                    self.peers[&bot].player.eye(),
                    at + Vec3::Y,
                    brain.kind.sight,
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
                if let Some(mut o) = self.bot_opportunity(bot, v, resource, enemy.at, tick) {
                    if brain.kind.behaviours.get("chase").copied().unwrap_or(1.0) == 0.0
                        && feet.distance(o.point) > 2.0
                    {
                        continue;
                    }
                    o.utility -= feet.distance(o.point) * 0.002;
                    if best.is_none_or(|old| o.utility > old.utility) {
                        best = Some(o);
                    }
                }
            }
        }
        self.bots.brains.get_mut(&bot)?.object_cursor = (start + visited.max(1)) % len;
        let opportunity = best?;
        if self.bots.claims.acquire(
            bot,
            enemy.subject,
            opportunity.resource,
            feet.distance(opportunity.point),
            tick,
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
                brain.vehicle_stuck = 0;
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
                let Some(v) = self.bots.objects.iter().find(|v| v.id.0 == vehicle) else {
                    return Ok(None);
                };
                let Some(enemy) = enemy else {
                    return Ok(None);
                };
                let Some(o) = self.bot_opportunity(bot, v, claim.resource, enemy, tick) else {
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
    pub(super) fn bot_walk_direction(&self, bot: OwnerId, desired: Vec3) -> Vec3 {
        if desired == Vec3::ZERO || self.seated(bot) {
            return desired;
        }
        let own = &self.peers[&bot].player;
        let feet = Vec3::from(own.state().feet);
        let blocked = self.peers.iter().any(|(o, p)| {
            if *o == bot || !p.combat.alive || self.seated(*o) || !self.bot_allies(bot, *o) {
                return false;
            }
            let delta = flat(Vec3::from(p.player.state().feet) - feet);
            let along = delta.dot(desired);
            let width = (own.tuning().width + p.player.tuning().width) * 0.5;
            along > 0.0 && along < width + 0.6 && (delta - desired * along).length() < width
        });
        if blocked {
            (desired * 0.25 + Vec3::new(-desired.z, 0.0, desired.x)).normalize_or_zero()
        } else {
            desired
        }
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

    pub(super) fn bot_fire_clear(
        &self,
        bot: OwnerId,
        origin: Vec3,
        target: Vec3,
        splash: f32,
    ) -> bool {
        let delta = target - origin;
        let length = delta.length();
        if length < 0.01 {
            return false;
        }
        let direction = delta / length;
        !self.peers.iter().any(|(o, p)| {
            if *o == bot || !p.combat.alive || !self.bot_allies(bot, *o) {
                return false;
            }
            if self.mounted(bot).is_some()
                && self.mounted(bot).map(|(v, _)| v) == self.mounted(*o).map(|(v, _)| v)
            {
                return false;
            }
            let centre =
                Vec3::from(p.player.state().feet) + Vec3::Y * p.player.tuning().stand_height * 0.5;
            let along = (centre - origin).dot(direction).clamp(0.0, length);
            centre.distance(origin + direction * along)
                < p.player.tuning().stand_height * 0.5 + splash
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
        let role = &d.seats[usize::from(seat)];
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
                brain.vehicle_anchor = None;
                brain.vehicle_stuck = 0;
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
            let toward = next
                .filter(|p| p.through.is_none())
                .map(|p| flat(p.feet - at));
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
            // It arrives once its side passes the point.
            let reach = (d.bounds_max[0] - d.bounds_min[0]) * v.scale * 0.5;
            let gear = match toward {
                _ if brain.vehicle_stuck > 360 && brain.vehicle_stuck < 480 => {
                    crate::route::Gear::Reverse { nose: false }
                }
                Some(delta) => {
                    crate::route::gear(turn, reach, cruise, error, delta.length(), brain.drive_gear)
                }
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
            let corner_speed =
                crate::route::pace(turn, gear, heading_error, distance, cruise).max(2.0);
            let arrival_speed = (2.0 * braking * distance).sqrt().max(2.0);
            let brake =
                signed_speed * travel_sign < -0.5 || speed > corner_speed.min(arrival_speed);
            input.forward = if toward.is_none() || hazard || waiting || brake {
                0.0
            } else {
                travel_sign * (1.0 - heading_error.abs() * 0.3).clamp(0.25, 0.8)
            };
            input.jump = input.forward == 0.0;
            // The leave leg (`route`): a chassis that cannot hurt the one it
            // chases (no gun, and no runover for someone not on foot) only
            // carries the bot there. Once its side is about as close as the
            // bot fights from on foot, it stops and gets out.
            let target = self.bots.brains[&bot].target.and_then(|t| {
                self.peers
                    .get(&t)
                    .map(|p| (self.mounted(t).is_some(), Vec3::from(p.player.state().feet)))
            });
            let leave = target.is_some_and(|(mounted, enemy)| {
                matches!(behaviour, Behaviour::Chase | Behaviour::Fight)
                    && d.weapon.is_none()
                    && (mounted || d.runover_damage <= 0.0)
                    && flat(enemy - at).length()
                        <= (d.bounds_max[2] - d.bounds_min[2]) * v.scale * 0.5 + LEAVE_REACH
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
            let progressed = brain
                .vehicle_anchor
                .is_none_or(|old| flat(at - old).length() >= 0.5);
            if progressed || waiting || !pursuing {
                brain.vehicle_anchor = Some(at);
                brain.vehicle_stuck = 0;
            } else {
                // Braking for an impassable route is also a lack of progress.
                // Throttle and contact impulses alone cannot renew a task.
                brain.vehicle_stuck += 1;
            }
            if brain.vehicle_stuck > 360 && brain.vehicle_stuck < 480 && !hazard {
                input.forward = -0.5;
                input.jump = false;
            }
            if brain.vehicle_stuck >= 360 && brain.vehicle_stuck.is_multiple_of(120) {
                brain.plan.clear();
                brain.search = None;
                brain.settled = false;
            }
            if brain.vehicle_stuck >= 720 {
                brain.next_interaction = tick + RETRY;
                brain.vehicle_stuck = 0;
                let _ = self.dismount_vehicle(bot);
                input = MoveInput {
                    yaw: self.bots.brains[&bot].yaw,
                    ..Default::default()
                };
            }
            if leave && speed < 3.0 {
                let brain = self.bots.brains.get_mut(&bot).unwrap();
                brain.next_interaction = tick + RETRY;
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
                self.bots.brains.get_mut(&bot).unwrap().next_interaction = tick + RETRY;
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
        if behaviour == Behaviour::Wander && tick.is_multiple_of(CREW_WAIT) {
            let _ = self.dismount_vehicle(bot);
        }
        Ok(input)
    }
}
