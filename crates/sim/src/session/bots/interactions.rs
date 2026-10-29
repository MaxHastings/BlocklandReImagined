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
const DISMOUNT_BEAT: u64 = 360;
const OBJECTS_PER_BOT: usize = 8;
const LOOKAHEAD_POINTS: usize = 24;
/// How far past its own half-length a driver that only carries its bot to
/// an enemy stops and gets out: about where the bot fights from on foot.
const LEAVE_REACH: f32 = 3.0;
/// How far short of an objective's point (past its standoff) a driver taken
/// there pulls up and gets out to walk the rest.
const OBJECTIVE_WALK: f32 = 6.0;
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

/// What a bot wants an object for: the same seat serves a fight, a look
/// for one, or idle play, and a body is pushed only at a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Want {
    /// Going after an enemy it knows of.
    Fight,
    /// Going to look for one (`explore`): a seat is a way there.
    Explore,
    /// Going where an objective's step needs only its feet: a seat is a way
    /// there, worth it only when the drive beats the walk.
    Objective,
    /// Idle play near a player in sight (`extras`).
    Play,
}

/// The space a shot from `origin` at `target` sweeps: on `past` beyond,
/// `radius` wide; none for a shot too short to judge.
pub(super) fn shot_space(
    origin: Vec3,
    target: Vec3,
    radius: f32,
    past: f32,
) -> Option<super::claims::Space> {
    let delta = target - origin;
    let length = delta.length();
    (length >= 0.01).then(|| super::claims::Space {
        from: origin,
        to: origin + delta / length * (length + past),
        radius,
        spread: 0.0,
    })
}

/// Slower than this a projectile is lying still, waiting to go off.
const RESTING_SPEED: f32 = 1.0;
impl Session {
    /// A passenger may have boarded the reachable lower seat of a tall vehicle.
    /// Fill its useful empty role through the same seat keys players use.
    pub(super) fn promote_bot_seat(&mut self, bot: OwnerId) -> Result<()> {
        if !self.is_alive(bot)
            || self
                .bots
                .brains
                .get(&bot)
                .is_none_or(|b| b.resting || b.kind.weight("interact") <= 0.0)
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

    /// The other role of the vehicle `bot` crews alone, free for it to take:
    /// its gun seat from its controls, or its controls from its gun, as a
    /// player takes either with the seat keys. A seat someone holds, or
    /// one an ally is on its way to, is not free; nor is a gun that cannot
    /// fire. Returns that seat and whether it is the gun's.
    pub(super) fn bot_lone_role(&self, bot: OwnerId, tick: u64) -> Option<(usize, bool)> {
        let (vehicle, seat) = self.mounted(bot)?;
        let world = self.vehicles.world.as_ref()?;
        let d = world.definition_of(VehicleId(vehicle))?;
        let (controls, gun) = (d.control_seat()?, d.weapon_seat()?);
        let (other, to_gun) = match usize::from(seat) {
            s if s == controls && gun != controls => (gun, true),
            s if s == gun && gun != controls => (controls, false),
            _ => return None,
        };
        let free = world.seat_occupant(VehicleId(vehicle), other).is_none()
            && !self.bot_claimed(
                Resource::Seat {
                    vehicle,
                    seat: other as u8,
                },
                bot,
                tick,
            )
            && (!to_gun || self.vehicle_gun(vehicle).is_some());
        free.then_some((other, to_gun))
    }

    /// The gun a seat away from the controls `bot` holds alone
    /// (`bot_lone_role`): its weapon too, one switch off.
    pub(super) fn bot_spare_gun(&self, bot: OwnerId, tick: u64) -> Option<Weapon> {
        let (vehicle, _) = self.mounted(bot)?;
        self.bot_lone_role(bot, tick)
            .filter(|(_, to_gun)| *to_gun)
            .and_then(|_| self.vehicle_gun(vehicle))
    }

    /// Whether the chassis `bot` sits in runs `target` over (on foot, a
    /// chassis that hurts on contact, the rules letting it hurt them) from
    /// nearer than `near`: inside a gun's near edge, where its own blast
    /// would reach it, the chassis is the blow.
    pub(super) fn bot_runs_over_near(&self, bot: OwnerId, target: OwnerId, near: f32) -> bool {
        let Some((vehicle, _)) = self.mounted(bot) else {
            return false;
        };
        let (Some(world), Some(them)) = (self.vehicles.world.as_ref(), self.peers.get(&target))
        else {
            return false;
        };
        let (Some(d), Some(v)) = (
            world.definition_of(VehicleId(vehicle)),
            world.vehicle_snapshot(&self.simulation.physics, VehicleId(vehicle)),
        ) else {
            return false;
        };
        let at = Vec3::from(v.transform.position);
        d.runover_damage > 0.0
            && self.mounted(target).is_none()
            && self.can_damage_player(bot, target, false)
            && self.bot_vehicle_reaches_player(&v, target, Vec3::from(them.player.state().feet))
            && flat(Vec3::from(them.player.state().feet) - at).length() < near
    }

    /// A horizontal drive cannot strike a body above or below the real
    /// chassis. Use its instantiated collider heights and the target's actual
    /// posture/scale, not visual bounds or the gun's horizontal near band.
    /// `feet` is the caller's observation, including dated seat opportunities.
    fn bot_vehicle_reaches_player(&self, v: &VehicleSnapshot, target: OwnerId, feet: Vec3) -> bool {
        let (Some((bottom, top)), Some(peer)) =
            (self.bot_vehicle_heights(v), self.peers.get(&target))
        else {
            return false;
        };
        let state = peer.player.state();
        let head = feet.y + crate::water::body_height(state, peer.player.tuning()) * state.scale;
        top > feet.y && bottom < head
    }

    fn bot_vehicle_heights(&self, v: &VehicleSnapshot) -> Option<(f32, f32)> {
        self.vehicles
            .world
            .as_ref()?
            .colliders_of(&self.simulation.physics, v.id)
            .into_iter()
            .map(|handle| {
                let bounds = self.simulation.physics.colliders[handle].compute_aabb();
                (bounds.mins.y, bounds.maxs.y)
            })
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
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
            self.bots.claims.release_interaction(o);
        }
    }

    /// Whether an ally holding `vehicle`'s controls offers its free seats
    /// (`team_seats`): a call heard as far as the bot sees, as its alerts
    /// are.
    fn bot_offered(&self, bot: OwnerId, vehicle: u64, tick: u64) -> bool {
        self.bots
            .claims
            .intents(tick)
            .any(|(o, i)| i.seats == Some(vehicle) && o != bot && self.bot_allies(bot, o))
    }

    /// How far off `bot` looks for an object to use: near by, or as far
    /// as it sees for a seat an ally offers.
    fn bot_object_reach(&self, bot: OwnerId, offered: bool) -> f32 {
        if offered {
            self.bot_sight_reach(&self.bots.brains[&bot].kind)
        } else {
            DISCOVER
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

    /// Whether an ally of `bot` is on its way aboard `vehicle`: its claim
    /// on a seat lives (it keeps closing on it, `claims`) and taking it up
    /// is still what it does. A claim left over from a choice since
    /// changed holds nobody up.
    fn bot_coming_aboard(&self, bot: OwnerId, vehicle: u64, tick: u64) -> bool {
        self.bots.claims.intents(tick).any(|(other, intent)| {
            other != bot
                && intent.option == Behaviour::Interact as u8
                && self.bot_allies(bot, other)
                && self
                    .bots
                    .claims
                    .owner_claim(other, tick)
                    .is_some_and(|c| c.resource.vehicle() == vehicle)
        })
    }

    /// Assembly has one deadline per ride, not a fresh wait for every new
    /// claimant. Otherwise a succession of nearby bots parks a driver forever.
    pub(super) fn bot_assembling_crew(&self, bot: OwnerId, tick: u64) -> bool {
        self.bots.brains[&bot]
            .vehicle_since
            .is_none_or(|(_, since)| tick < since + DISMOUNT_BEAT)
    }

    /// Whether the teammate bot at `v`'s controls goes `bot`'s way: where
    /// its intent takes it (its target, else its place) is nearer `to`
    /// than the vehicle is now. None when no bot drives it, or its intent
    /// says nowhere: a person at the wheel is trusted to.
    fn bot_driver_goes_toward(&self, v: &VehicleSnapshot, to: Vec3, tick: u64) -> Option<bool> {
        let world = self.vehicles.world.as_ref()?;
        let d = world.definition(&v.definition)?;
        let driver = world.seat_occupant(v.id, d.control_seat()?)?.owner.0;
        let (_, intent) = self.bots.claims.intents(tick).find(|(o, _)| *o == driver)?;
        let heads = match intent.target {
            Some(super::claims::Target::Player(p)) => {
                Some(Vec3::from(self.peers.get(&p)?.player.state().feet))
            }
            Some(super::claims::Target::Object(id)) => self
                .bots
                .objects
                .iter()
                .find(|o| o.id.0 == id)
                .map(|o| Vec3::from(o.transform.position)),
            None => intent.place,
        }?;
        let at = Vec3::from(v.transform.position);
        Some(flat(heads - to).length() < flat(at - to).length())
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
        want: Want,
        tick: u64,
    ) -> Option<Opportunity> {
        let idle = want == Want::Play;
        let peer = self.peers.get(&bot)?;
        let feet = Vec3::from(peer.player.state().feet);
        let world = self.vehicles.world.as_ref()?;
        let d = world.definition(&v.definition)?;
        let offered = self.bot_offered(bot, v.id.0, tick);
        if v.destroyed
            || self.bot_claimed(resource, bot, tick)
            || feet.distance(Vec3::from(v.transform.position)) > self.bot_object_reach(bot, offered)
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
                    // A moving one is boarded once it stops: one its
                    // driver offers stops for whoever claims a seat.
                    || Vec3::from(v.velocity).length() > 2.0
                        && !offered
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
                // Only the seats its kind takes (`BotKind::vehicles`).
                let takes = &self.bots.brains[&bot].kind.vehicles;
                let allowed = if role.controls {
                    takes.drive
                } else if role.weapon {
                    takes.gun
                } else {
                    takes.ride
                };
                if !allowed {
                    return None;
                }
                let crew = self.bot_crew(bot, v, tick);
                // Complete roles before filling passenger seats. An empty
                // vehicle needs someone at its controls before a passenger
                // has a reason to board it.
                // For an objective only the controls take it there.
                if want == Want::Objective && !role.controls {
                    return None;
                }
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
                } else if crew
                    && feet.distance(enemy) > 12.0
                    // A passenger goes where its driver takes it: only a
                    // driver going its way is worth riding with.
                    && self.bot_driver_goes_toward(v, enemy, tick) != Some(false)
                {
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
                let runs_over = want == Want::Fight
                    && d.runover_damage > 0.0
                    && self.mounted(subject).is_none()
                    && self.can_damage_player(bot, subject, false)
                    && self.bot_vehicle_reaches_player(v, subject, enemy);
                let cruise = if runs_over {
                    d.max_speed
                } else {
                    d.max_speed * crate::route::CRUISE
                };
                // A blow is struck sooner from a seat nearer than the enemy.
                let strikes = runs_over && feet.distance(point) < feet.distance(enemy);
                if !idle
                    && (d.weapon.is_none() || want == Want::Objective)
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
                if matches!(want, Want::Explore | Want::Objective)
                    || d.is_actor()
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
                    v,
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
                let base = if idle { IDLE } else { 0.85 };
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
    /// `objective_to`: where an objective that needs only its feet takes it;
    /// a seat is a way there (its own subject, as a place to explore), ahead
    /// of a fight on the way.
    pub(super) fn bot_interaction(
        &mut self,
        bot: OwnerId,
        enemy: Option<Knowledge>,
        seats: bool,
        objective_to: Option<Vec3>,
        tick: u64,
    ) -> Option<Opportunity> {
        let seats = seats || objective_to.is_some();
        let brain = &self.bots.brains[&bot];
        if self.seated(bot) || tick < brain.next_interaction || brain.kind.weight("interact") == 0.0
        {
            self.bots.claims.release_interaction(bot);
            return None;
        }
        // An enemy known comes first, as it did; else on the way to an
        // objective, a seat is a way there; else a way to where it would
        // look for an enemy (`explore`, its own subject); with no enemy
        // about, idle play toward a player in sight (`extras`).
        let exploring = brain.explore.to.filter(|_| seats && !brain.tethered());
        let (subject, toward, want) = match (enemy, exploring, objective_to) {
            (Some(enemy), _, _) => (enemy.subject, enemy.at, Want::Fight),
            (None, _, Some(to)) => (bot, to, Want::Objective),
            (None, Some(to), None) => (bot, to, Want::Explore),
            (None, None, None) => match self.bot_idle_mark(bot, tick) {
                Some((mark, at)) => (mark, at, Want::Play),
                None => {
                    self.bots.claims.release_interaction(bot);
                    return None;
                }
            },
        };
        let brain = &self.bots.brains[&bot];
        let feet = Vec3::from(self.peers[&bot].player.state().feet);
        if let Some(claim) = self.bots.claims.owner_claim(bot, tick) {
            // Wanted for something else now (another enemy, a look round,
            // an objective's step): handed back, not failed.
            let seat = matches!(claim.resource, Resource::Seat { .. });
            let seat_only = matches!(want, Want::Explore | Want::Objective);
            if claim.subject != subject || seat && !seats || !seat && seat_only {
                self.bots.claims.release_interaction(bot);
            }
        }
        if let Some(claim) = self.bots.claims.owner_claim(bot, tick) {
            let opportunity = self
                .bots
                .objects
                .iter()
                .find(|v| v.id.0 == claim.resource.vehicle())
                .and_then(|v| {
                    self.bot_opportunity(bot, v, claim.resource, (subject, toward), want, tick)
                });
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
            let reach = self.bot_object_reach(bot, self.bot_offered(bot, v.id.0, tick));
            if v.destroyed || feet.distance(at) > reach {
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
                    self.bot_sight_reach(&brain.kind),
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
                .filter(|r| want != Want::Objective || matches!(r, Resource::Seat { .. }))
            {
                if let Some(mut o) =
                    self.bot_opportunity(bot, v, resource, (subject, toward), want, tick)
                {
                    if brain.kind.weight("chase") == 0.0 && feet.distance(o.point) > 2.0 {
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
        source: &VehicleSnapshot,
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
                && self.bot_vehicle_reaches_player(source, *o, Vec3::from(p.player.state().feet))
                && within(
                    Vec3::from(p.player.state().feet),
                    p.player.tuning().width * 0.5,
                )
        }) || self.bots.objects.iter().any(|v| {
            self.mounted(bot).is_none_or(|(id, _)| id != v.id.0)
                && self
                    .bot_vehicle_heights(source)
                    .zip(self.bot_vehicle_heights(v))
                    .is_none_or(|(a, b)| a.1 > b.0 && a.0 < b.1)
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
                    self.bots.claims.release_interaction(bot);
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
                    if idle { Want::Play } else { Want::Fight },
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
        let (own_min, own_max) = own.world_bounds();
        let ahead = |p: &super::Peer| {
            let (other_min, other_max) = p.player.world_bounds();
            if other_min[1] >= own_max[1] || other_max[1] <= own_min[1] {
                return false;
            }
            let at = Vec3::from(p.player.state().feet);
            let delta = flat(at - feet);
            let along = delta.dot(desired);
            let width = (own.tuning().width + p.player.tuning().width) * 0.5;
            along > 0.0 && along < width + 0.6 && (delta - desired * along).length() < width
        };
        let side = Vec3::new(-desired.z, 0.0, desired.x);
        let waist = Vec3::new(feet.x, (own_min[1] + own_max[1]) * 0.5, feet.z);
        // A gap: something solid close on both sides. Only a kind with
        // teamwork files through one behind an ally.
        let teamwork = self
            .bots
            .brains
            .get(&bot)
            .is_some_and(|b| b.kind.team.teamwork > 0.0);
        let sides = [side, -side].map(|d| {
            teamwork
                .then(|| self.world_ray(waist, d, own.tuning().width + 0.6))
                .flatten()
        });
        let no_room = sides.iter().all(Option::is_some);
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
            .map(|(id, p)| (*id, flat(Vec3::from(p.player.state().feet) - feet)))
            .min_by(|a, b| a.1.dot(desired).total_cmp(&b.1.dot(desired)));
        if let Some((other, _)) = blocker
            && no_room
            && self.bot_allies(bot, other)
        {
            let peer = &self.peers[&other];
            let pass_width = own.tuning().width + peer.player.tuning().width;
            if sides[0].unwrap() + sides[1].unwrap() <= pass_width {
                let at = Vec3::from(peer.player.state().feet);
                let oncoming = self.bots.brains.get(&other).is_some_and(|brain| {
                    brain
                        .plan
                        .first()
                        .is_some_and(|next| flat(next.feet - at).dot(desired) < 0.0)
                });
                // Neither body can pass laterally. One consistent member of
                // an oncoming pair advances; the other backs along the file.
                // A human or stationary ally cannot be assumed to cooperate,
                // so this bot yields. Safety still checks the backward step.
                return if oncoming && bot < other {
                    desired
                } else {
                    -desired
                };
            }
        }
        match blocker {
            // One already off to its left is passed on its right, not
            // walked across; one dead ahead on its left.
            Some((_, delta)) if delta.dot(side) > own.tuning().width * 0.1 => {
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
    pub(super) fn bot_safe_drop(&self, bot: OwnerId) -> Option<f32> {
        let peer = self.peers.get(&bot)?;
        let state = peer.player.state();
        let gravity = peer.player.tuning().gravity.max(1.0);
        let impact = crate::player_types::PlayerType::from_archetype(state.archetype)
            .unwrap_or_default()
            .min_impact_speed()
            * state.scale.max(1.0);
        Some(impact * impact / (2.0 * gravity))
    }
    /// The live projectiles whose blast would reach a body of `bot`'s
    /// standing at `feet` (grown by its half width) and could hurt it (its
    /// own, an ally's or an enemy's; `Session::can_damage_player`): a
    /// grenade lying where it waits to go off. One flying off away from it
    /// is not waiting for it.
    fn bot_live_blasts(
        &self,
        bot: OwnerId,
        feet: Vec3,
        body: &crate::nav::Body,
    ) -> impl Iterator<Item = Vec3> + '_ {
        let centre = feet + Vec3::Y * body.height * 0.5;
        let width = body.width;
        self.weapons.projectiles().filter_map(move |p| {
            let d = self.weapons.pack.projectiles.get(&p.definition)?;
            let reach = d.explosion.radius * p.scale + width * 0.5;
            // One lying still counts however its last jitter points (a
            // resting grenade's velocity flips about zero with gravity).
            (d.explosion.damage > 0.0
                && (p.velocity.length() < RESTING_SPEED
                    || p.velocity.dot(p.position - centre) <= 0.0)
                && centre.distance(p.position) < reach
                && self.can_damage_player(p.source.0, bot, true))
            .then_some(p.position)
        })
    }
    /// The way straight out of the nearest live blast `bot` stands in at
    /// `feet` (`bot_live_blasts`), if it stands in one: the dodge's to take.
    pub(super) fn bot_blast_escape(
        &self,
        bot: OwnerId,
        feet: Vec3,
        body: &crate::nav::Body,
    ) -> Option<Vec3> {
        let centre = feet + Vec3::Y * body.height * 0.5;
        let at = self
            .bot_live_blasts(bot, feet, body)
            .min_by(|a, b| a.distance(centre).total_cmp(&b.distance(centre)))?;
        Some(flat(centre - at).normalize_or_zero()).filter(|away| *away != Vec3::ZERO)
    }
    /// Whether a step along `toward` from `feet` takes `bot` into a live
    /// blast, or no further out of the one it stands in: safety refuses it.
    pub(super) fn bot_blast_refuses(
        &self,
        bot: OwnerId,
        feet: Vec3,
        body: &crate::nav::Body,
        toward: Vec3,
    ) -> bool {
        match self.bot_blast_escape(bot, feet, body) {
            Some(away) => toward.dot(away) <= 0.0,
            None => self
                .bot_live_blasts(bot, feet + toward * body.width, body)
                .next()
                .is_some(),
        }
    }
    pub(super) fn bot_fall_ahead(
        &self,
        bot: OwnerId,
        feet: Vec3,
        body: &crate::nav::Body,
        toward: Vec3,
        off_route: bool,
        grounded: bool,
    ) -> bool {
        let Some(hurts) = self.bot_safe_drop(bot) else {
            return false;
        };
        // Off its route (footwork, a strafe, a goof's walk) it keeps to
        // floor it can walk back up from, on its feet or in the air (a
        // jump in place): measured from the floor under it, not from its
        // feet in the air, from where any floor at all would do. A route
        // plans its own drops, so on it no drop the route may take is
        // refused, nor one that does not hurt.
        let depth = if off_route {
            // Standing, its feet are on that floor. In the air it is the
            // highest floor under any part of its body (its middle may be
            // out over a rim it jumped from).
            let reach = body.width * 0.4;
            let above = if grounded {
                0.0
            } else {
                [
                    Vec3::ZERO,
                    Vec3::X * reach,
                    -Vec3::X * reach,
                    Vec3::Z * reach,
                    -Vec3::Z * reach,
                ]
                .into_iter()
                .filter_map(|side| {
                    self.world_ray(feet + side + Vec3::Y * 0.5, Vec3::NEG_Y, 0.5 + hurts)
                })
                .reduce(f32::min)
                .map_or(0.0, |d| (d - 0.5).max(0.0))
            };
            above + body.step + body.drop
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
        let depth = ((feet.y - toward.y).max(0.0) + body.step + 0.5).min(body.drop);
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

    /// An empty route at a stationary, ungrounded pose is not normal flight.
    /// Only after a full no-motion window, choose nearby safe floor using
    /// the existing step-off checks. Never steer a real fall or active jets.
    pub(super) fn bot_stranded_step(
        &self,
        bot: OwnerId,
        feet: Vec3,
        body: &crate::nav::Body,
        state: &crate::player::PlayerState,
        tick: u64,
    ) -> Option<Vec3> {
        let brain = self.bots.brains.get(&bot)?;
        let (stopped, since) = brain.path_stop?;
        if self.seated(bot)
            || state.grounded
            || state.jetting
            || Vec3::from(state.velocity).length_squared() > 0.25
            || feet.distance(stopped) > 0.2
            || tick < since.saturating_add(120)
            || brain.search.is_some()
        {
            return None;
        }
        let goal = brain.goal?.point(brain.home);
        self.bot_step_off(
            bot,
            feet,
            body,
            Vec3::new(goal.x, feet.y - body.drop, goal.z),
            feet.y,
        )
    }

    /// A vehicle is a body the walk grid leaves out (`nav`): one a walking
    /// bot is about to walk into is walked round by its nearer side, not
    /// pressed against (which pushes it, `movables`) or hopped onto. Not the
    /// one it stands on or is inside, makes for (`goal`, a seat beside it)
    /// or whose crew is its `quarry`.
    pub(super) fn bot_vehicle_detour(
        &mut self,
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
            let side = self.bots.brains[&bot]
                .vehicle_detour
                .filter(|(id, _)| *id == v.id.0)
                .map_or_else(
                    || {
                        if desired.cross(centre - feet).y > 0.0 {
                            1.0
                        } else {
                            -1.0
                        }
                    },
                    |(_, side)| side,
                );
            self.bots.brains.get_mut(&bot).unwrap().vehicle_detour = Some((v.id.0, side));
            // Follow the body boundary on that side. A tangent to each new
            // grid direction can point back the way it just walked, producing
            // a repeated side-to-side shuffle at the same hull or seat.
            let inward = flat(centre - feet).normalize_or_zero();
            let tangent = Vec3::new(inward.z, 0.0, -inward.x) * side;
            return (desired * 0.25 + tangent).normalize_or_zero();
        }
        self.bots.brains.get_mut(&bot).unwrap().vehicle_detour = None;
        desired
    }

    /// Local execution uses the actual oriented chassis, above its wheel
    /// contact plane. Static navigation is only a corridor proposal.
    /// Whether anything stands on the path a driven chassis rolls along
    /// for `length`, setting out along `travel` and turning by `curvature`
    /// (radians a unit, about up): its hull swept along the arc in chords
    /// no more than `ARC_STEP` apart, and allies on foot within
    /// `(length, radius)` of it.
    fn bot_vehicle_path_blocked(
        &self,
        bot: OwnerId,
        v: &VehicleSnapshot,
        travel: Vec3,
        curvature: f32,
        length: f32,
        (ally_length, radius): (f32, f32),
    ) -> bool {
        // Each chord: where it starts (from here), the hull's turn there,
        // and the way and length it runs.
        let chords = |length: f32| {
            let n = (curvature.abs() * length / ARC_STEP).ceil().max(1.0) as usize;
            let step = length / n as f32;
            let turn = |along: f32| glam::Quat::from_rotation_y(curvature * along);
            let mut from = Vec3::ZERO;
            let mut chords = Vec::with_capacity(n);
            for i in (0..n).filter(|_| step > 0.0) {
                let along = turn((i as f32 + 0.5) * step) * travel;
                chords.push((from, turn(i as f32 * step), along, step));
                from += along * step;
            }
            chords
        };
        let at = Vec3::from(v.transform.position);
        chords(ally_length).into_iter().any(|(from, _, way, step)| {
            self.bot_ally_corridor(bot, v, at + from, way, step, radius)
        }) || chords(length)
            .into_iter()
            .any(|(from, turn, way, step)| !self.bot_hull_clear(bot, v, (from, turn), way, step))
    }

    /// Whether a chassis whose middle reaches `at`, rolling `way`, would
    /// meet a drop on the way there or under its leading end (the edge of
    /// the world, a cliff): the drive's own "never off a fall" rule, as a
    /// walker's is (`bot_safe_walk`).
    ///
    /// A drop is the floor falling away faster than a slope can: the floor
    /// is followed from under the hull in steps of half its length (a gap
    /// shorter than that it bridges). Each step's floor may rise or fall
    /// by the step's run (no chassis drives a grade steeper than one in
    /// one, and a wall is the path check's), and fall a further step as
    /// deep as the chassis is tall, a kerb it rolls off. So a hill, up or
    /// down, is floor, and a cliff past it is still a drop.
    fn bot_vehicle_edge(&self, v: &VehicleSnapshot, at: Vec3, way: Vec3) -> bool {
        let Some(d) = self
            .vehicles
            .world
            .as_ref()
            .and_then(|w| w.definition(&v.definition))
        else {
            return false;
        };
        let size = (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)) * v.scale;
        let lead = at + way.normalize_or_zero() * size.z * 0.5;
        let support = d
            .wheels
            .iter()
            .map(|w| (w.position[1] - w.radius - w.rest_length) * v.scale)
            .fold(d.bounds_min[1] * v.scale, f32::min);
        let hull = Vec3::from(v.transform.position);
        let run = flat(lead - hull);
        let steps = (run.length() / (size.z * 0.5).max(0.1)).ceil().max(1.0);
        let step = run.length() / steps;
        let mut floor = hull.y + support;
        for i in 1..=steps as usize {
            let over = hull + run * (i as f32 / steps);
            let top = floor + step + size.y;
            let Some(down) = super::super::admin_players::world_ray(
                &self.simulation,
                Vec3::new(over.x, top, over.z),
                Vec3::NEG_Y,
                2.0 * (step + size.y),
            ) else {
                return true;
            };
            floor = top - down;
        }
        false
    }

    fn bot_vehicle_clear(
        &self,
        bot: OwnerId,
        v: &VehicleSnapshot,
        direction: Vec3,
        reach: f32,
    ) -> bool {
        self.bot_hull_clear(bot, v, (Vec3::ZERO, glam::Quat::IDENTITY), direction, reach)
    }

    /// Whether the hull of `v`, moved by `offset` (a shift, and a turn
    /// about its centre) from where it is, sweeps clear `reach` along
    /// `direction`.
    fn bot_hull_clear(
        &self,
        bot: OwnerId,
        v: &VehicleSnapshot,
        offset: (Vec3, glam::Quat),
        direction: Vec3,
        reach: f32,
    ) -> bool {
        let Some(world) = self.vehicles.world.as_ref() else {
            return false;
        };
        let origin = Vec3::from(v.transform.position);
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
        let query = self.simulation.physics.query_pipeline_with_filter(
            QueryFilter::default()
                .exclude_sensors()
                .predicate(&predicate),
        );
        let cast = |pose: Pose, shape: &dyn Shape| {
            let hit = query.cast_shape(
                &pose,
                direction * reach,
                shape,
                ShapeCastOptions {
                    max_time_of_impact: 1.0,
                    // Existing support/contact is not a new obstruction when
                    // travelling tangent to it or out of it. Physics owns contact.
                    stop_at_penetration: false,
                    ..Default::default()
                },
            );
            hit.is_none()
        };
        let colliders = world.colliders_of(&self.simulation.physics, v.id);
        !colliders.is_empty()
            && colliders.into_iter().all(|handle| {
                let collider = &self.simulation.physics.colliders[handle];
                let current = collider.position();
                let pose = Pose {
                    translation: origin + offset.0 + offset.1 * (current.translation - origin),
                    rotation: offset.1 * current.rotation,
                    ..Default::default()
                };
                // Use exactly the instantiated collision geometry, including
                // scale, body offsets and the independently posed attachment.
                // Visual bounds fill empty corners beside a tapered hull.
                if let Some(compound) = collider.shape().as_compound() {
                    compound
                        .shapes()
                        .iter()
                        .all(|(local, shape)| cast(pose * *local, shape.as_ref()))
                } else {
                    cast(pose, collider.shape())
                }
            })
    }

    pub(super) fn bot_crew_ready(&self, bot: OwnerId, tick: u64) -> bool {
        let Some((vehicle, _)) = self.mounted(bot) else {
            return true;
        };
        let since = self.bots.brains[&bot]
            .vehicle_since
            .map_or(tick, |(_, since)| since);
        if tick >= since + DISMOUNT_BEAT {
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
        let origin = Vec3::from(v.transform.position);
        let mut feet = origin
            + glam::Quat::from_array(v.transform.rotation) * Vec3::new(centre.x, support, centre.z);
        // Fully extended suspension is not the floor of a loaded vehicle.
        // Anchor the grid to the actual supporting world surface, including
        // a spawn plate under the chassis, rather than a point below it.
        let top = Vec3::new(feet.x, origin.y + 0.2, feet.z);
        if let Some(down) = self.world_ray(top, Vec3::NEG_Y, (top.y - feet.y).max(0.0) + 0.4) {
            feet.y = top.y - down;
        }
        Some((
            feet,
            Body {
                // The grid proposes a corridor for the vehicle's cross-section.
                // A square as wide as its diagonal rejects streets and parked
                // poses that the real oriented chassis fits. Actual posed hull
                // sweeps above remain authoritative for driving and turns.
                width: size.x.min(size.z),
                height: Vec3::from(d.bounds_max).y * v.scale - support,
                crouch_height: Vec3::from(d.bounds_max).y * v.scale - support,
                step: 0.2,
                jump: 0.2,
                crawl_jump: 0.2,
                crawl_step: 0.2,
                drop: 0.4,
                floor_cos: 0.85,
                conservative: true,
                bottom: (min.y - support).max(0.0),
                swims: false,
                climbs: false,
                // A chassis's own speed is its drive leg's: the search's
                // seconds only compare its routes with each other.
                motion: crate::route::Motion::of(&crate::player::PlayerTuning::default()),
                leaps: crate::reach::Leaps::none(),
            },
        ))
    }

    /// A legal parking pose can be outside the orientation-independent grid:
    /// its circumscribed square fills empty space beside the actual hull.
    /// Join that grid by a bounded straight forward/reverse leg checked with
    /// the instantiated collision shapes. Never narrow the grid or teleport
    /// its start through a wall to make a route appear possible.
    pub(super) fn bot_vehicle_departure(
        &self,
        bot: OwnerId,
        feet: Vec3,
        body: &Body,
        goal: Vec3,
    ) -> Option<Vec3> {
        let (id, _) = self.mounted(bot)?;
        let v = self
            .vehicles
            .world
            .as_ref()?
            .vehicle_snapshot(&self.simulation.physics, VehicleId(id))?;
        let sim = &self.simulation;
        let terrain = |o, d, r| sim.terrain_ray(o, d, r);
        let waters = sim.liquids();
        let ground = Ground {
            physics: &sim.physics,
            terrain: &terrain,
            passages: sim.passages(),
            waters: &waters,
            bodies: &[],
            motions: &[],
            solid: &crate::nav::all_solid,
        };
        if ground.fits(body, feet, false) {
            return None;
        }
        let forward =
            flat(glam::Quat::from_array(v.transform.rotation) * Vec3::NEG_Z).normalize_or_zero();
        let mut best: Option<(f32, Vec3)> = None;
        // At most 48 candidate floors; only clear grid entries need a hull
        // sweep. The chosen leg is retained until reached or the goal changes.
        for sign in [1.0, -1.0] {
            let way = forward * sign;
            for i in 1..=24 {
                let distance = body.width * i as f32 / 16.0;
                let at = feet + way * distance;
                let down = self.world_ray(
                    at + Vec3::Y * (body.step + 0.5),
                    Vec3::NEG_Y,
                    body.step + body.drop + 0.5,
                );
                let Some(down) = down else {
                    continue;
                };
                let at = at + Vec3::Y * (body.step + 0.5 - down);
                if !ground.fits(body, at, false) {
                    continue;
                }
                if !self.bot_vehicle_clear(bot, &v, way, distance)
                    || self.bot_vehicle_edge(
                        &v,
                        Vec3::from(v.transform.position) + way * distance,
                        way,
                    )
                {
                    break;
                }
                let score = flat(at - goal).length() + distance;
                if best.is_none_or(|(old, _)| score < old) {
                    best = Some((score, at));
                }
                break;
            }
        }
        best.map(|(_, at)| at)
    }

    /// On-demand route-start evidence for headless reports. This does not
    /// spend the live planner's budget or alter its searches/controls.
    pub fn bot_drive_diagnostic(&self, bot: OwnerId) -> Option<String> {
        let brain = self.bots.brains.get(&bot)?;
        let (feet, body) = self.bot_vehicle_body(bot)?;
        let terrain = |o, d, r| self.simulation.terrain_ray(o, d, r);
        let waters = self.simulation.liquids();
        let ground = Ground {
            physics: &self.simulation.physics,
            terrain: &terrain,
            passages: self.simulation.passages(),
            waters: &waters,
            bodies: &[],
            motions: &[],
            solid: &crate::nav::all_solid,
        };
        let mut nav = Nav::default();
        nav.unmetered();
        let node = nav.node_at(&ground, &body, feet);
        let neighbors = nav.closed_in(&ground, &body, feet);
        let departure = brain
            .goal
            .and_then(|g| self.bot_vehicle_departure(bot, feet, &body, g.point(brain.home)));
        Some(format!(
            "feet={feet:?} width={} height={} bottom={} fits={} node={node:?} neighbors={neighbors} retained={:?} proposed={departure:?}",
            body.width,
            body.height,
            body.bottom,
            ground.fits(&body, feet, false),
            brain.drive_departure
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
        // An affordance lost ends the drive (and any ride in it) at once: a
        // wreck, a wheeled hull on its side or its roof, or a chassis whose
        // stop could not be measured.
        let handling = (d.family == Family::Wheeled)
            .then(|| crate::reach::Handling::of(w, d, v.scale))
            .flatten();
        let lost = if v.destroyed {
            Some(Leave::Wrecked)
        } else if d.family != Family::Wheeled {
            None
        } else if handling.is_none() {
            Some(Leave::Undrivable)
        } else if d.wheeled_flight.is_none() && !crate::route::upright(v.transform.rotation) {
            Some(Leave::Overturned)
        } else {
            None
        };
        if let Some(why) = lost {
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.next_interaction = tick + GIVE_UP_TICKS;
            brain.plan.clear();
            brain.search = None;
            self.bot_leave_seat(bot, why);
            return Ok(MoveInput {
                yaw: self.bots.brains[&bot].yaw,
                ..Default::default()
            });
        }
        // A hull the next hit like the last would wreck is got out of, as
        // people bail out of a smoking tank: what it has left against the
        // last hit it was seen to take.
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        let last_hit = match brain.hull {
            Some((id, seen, hit)) if id == vehicle => {
                if v.damage > seen {
                    v.damage - seen
                } else {
                    hit
                }
            }
            _ => 0.0,
        };
        brain.hull = Some((vehicle, v.damage, last_hit));
        if last_hit > 0.0 && d.max_damage - v.damage <= last_hit {
            brain.next_interaction = tick + GIVE_UP_TICKS;
            brain.plan.clear();
            brain.search = None;
            self.bot_leave_seat(bot, Leave::BailOut);
            return Ok(MoveInput {
                yaw: self.bots.brains[&bot].yaw,
                ..Default::default()
            });
        }
        // Crewing it alone, it sits where its plan is served: at the gun to
        // fight what the gun reaches, at the controls to go somewhere or to
        // run down one too near for the gun's blast (`bot_lone_role`). With
        // nothing to do it stays put.
        if let Some((other, to_gun)) = self.bot_lone_role(bot, tick) {
            let target = self.bots.brains[&bot].target;
            let gun_serves = self.vehicle_gun(vehicle).is_some_and(|g| {
                !target.is_some_and(|t| self.bot_runs_over_near(bot, t, g.band().0))
            });
            let wants = match behaviour {
                Behaviour::Fight => Some(gun_serves),
                Behaviour::Chase
                | Behaviour::Search
                | Behaviour::Return
                | Behaviour::Objective
                | Behaviour::Explore => Some(false),
                _ => None,
            };
            if wants == Some(to_gun) {
                for _ in 0..d.seats.len() {
                    self.switch_seat(bot, 1)?;
                    if self.mounted(bot) == Some((vehicle, other as u8)) {
                        break;
                    }
                }
                return Ok(MoveInput {
                    yaw: self.bots.brains[&bot].yaw,
                    ..Default::default()
                });
            }
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
        // A passenger or gunner rides along while a teammate drives.
        let riding_along = !role.controls
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
        if brain.vehicle_since.is_none_or(|(id, _)| id != vehicle) {
            brain.vehicle_since = Some((vehicle, tick));
            brain.vehicle_headway.clear();
        }
        if let Some(handling) = handling.filter(|_| role.controls) {
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
                .filter(|p| {
                    p.combat.alive
                        && self.bot_vehicle_reaches_player(
                            &v,
                            p.player.state().owner,
                            Vec3::from(p.player.state().feet),
                        )
                })
                .map(|p| flat(Vec3::from(p.player.state().feet) - at));
            let toward = next
                .filter(|p| p.through.is_none())
                .map(|p| flat(p.feet - at))
                .or(strike);
            let error = toward.map_or(0.0, |d| wrap(yaw_to(d) - hull));
            // Pure pursuit that knows how tightly this chassis turns, as
            // measured (`reach::Handling`): a point inside its turning
            // circle is backed out of, never circled (`route::gear`). A
            // chassis that made no headway backs straight up for a while.
            let yaw_rate = -v.angular_velocity[1];
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            let turn = handling.tightest;
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
            // It arrives once its side passes the point.
            let arrive = (d.bounds_max[0] - d.bounds_min[0]) * v.scale * 0.5;
            let drive = crate::route::Driving {
                radius: turn,
                manoeuvre: handling.manoeuvre_speed(arrive),
                reach: arrive,
                cruise,
                behind: REVERSE_DEGREES.to_radians(),
                reverse_limit: if pursuing {
                    REVERSE_DISTANCE
                } else {
                    f32::INFINITY
                },
            };
            let gear = match toward {
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
            // A vehicle that steers toward its driver's look (`toward_look`)
            // ignores the mouse turn: the bot looks the way it would steer.
            // It looks past the lock for a full-lock turn: the chassis
            // holds its lock until the look comes round ahead.
            let looks = d.steering.toward_look;
            if looks {
                input.yaw = previous;
                input.head_yaw = (response_sign * (heading_error * 1.5 - yaw_rate * 0.25)).clamp(
                    -bri_motor::player::MAX_FREELOOK,
                    bri_motor::player::MAX_FREELOOK,
                );
            }
            input.pitch = 0.0;
            input.right = 0.0;
            // An ally coming for a seat is waited for as long as its claim
            // lives: while it keeps closing on the seat (`claims`).
            let waiting =
                self.bot_assembling_crew(bot, tick) && self.bot_coming_aboard(bot, vehicle, tick);
            // Where it rolls before it can stop (`Handling::stopping`),
            // along the arc it is turning on now: a hull in the way, or an
            // ally on foot, is braked for.
            let stopping = handling.stopping(speed);
            let travel = hull_forward * travel_sign;
            let curvature = (v.angular_velocity[1] / speed.max(f32::EPSILON))
                .clamp(-1.0 / handling.tightest, 1.0 / handling.tightest);
            let radius = (d.bounds_max[0] - d.bounds_min[0]) * v.scale * 0.5;
            // How far its end leads its middle the way it rolls (-z is the
            // nose): allies are kept clear of that end, not of its middle.
            let nose = if travel_sign > 0.0 {
                -d.bounds_min[2]
            } else {
                d.bounds_max[2]
            } * v.scale;
            let hazard = self.bot_vehicle_path_blocked(
                bot,
                &v,
                travel,
                curvature,
                stopping,
                (nose + stopping + ALLY_ROOM, radius),
            );
            // A chassis that runs people over is a blow on its way: where
            // it is about to drive, and what it sweeps before it can stop,
            // is its harm, for its side to keep out of as of any shot
            // (`team`). Parked with somewhere to go, the way it would pull
            // out is asked clear too: an ally standing there is told. The
            // ally coming aboard is not (`bot_in_the_way`): its way to the
            // seat runs through there, and the driver waits for it.
            self.bots.brains.get_mut(&bot).unwrap().sweep = (d.runover_damage > 0.0
                && (speed > STOPPED_SPEED || toward.is_some()))
            .then(|| super::claims::Harmed {
                way: super::claims::Space {
                    from: at,
                    to: at + travel * (nose + stopping + ALLY_ROOM),
                    radius,
                    spread: 0.0,
                },
                burst: None,
            });
            // A drop ahead of the way it rolls, within its stop or at its
            // end (`bot_vehicle_edge`), is braked for in a straight line,
            // as the stop was measured: braking on full lock slides on
            // toward it. A stop begun is carried through to rest: the stop
            // shrinks as it slows, and letting off then rolls it on over.
            // Half its width is added: met at a slant, an edge is reached
            // by a front corner before the middle of the nose.
            let rolling = hull_forward * response_sign;
            let ahead = self.bot_vehicle_edge(&v, at + rolling * (stopping + radius), rolling);
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            let edge = ahead || brain.edge_stop && speed > STOPPED_SPEED;
            brain.edge_stop = edge && speed > STOPPED_SPEED;
            if edge {
                if looks {
                    input.head_yaw = 0.0;
                } else {
                    input.yaw = wrap(previous - v.mouse_steering[0].clamp(-0.04, 0.04));
                }
            }
            let obstacle = hazard;
            let hazard = obstacle || edge;
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
                        self.mounted(t).is_none()
                            && d.runover_damage > 0.0
                            && self.bot_vehicle_reaches_player(&v, t, Vec3::from(state.feet)),
                        Vec3::from(state.feet),
                        Vec3::from(state.velocity),
                    )
                })
            });
            let standoff = (d.bounds_max[2] - d.bounds_min[2]) * v.scale * 0.5 + LEAVE_REACH;
            let carried = target.filter(|(runover, _, _)| {
                matches!(behaviour, Behaviour::Chase | Behaviour::Fight)
                    && d.weapon.is_none()
                    && !runover
            });
            let gap = carried.map(|(_, enemy, _)| flat(enemy - at).length() - standoff);
            let corner_speed =
                crate::route::pace(&handling, &drive, gear, heading_error, distance).max(2.0);
            // It stops where its route ends, not at each point it steers
            // by; a chassis that runs its quarry over is the blow, and is
            // not braked short of them.
            let end = if strike.is_some() {
                f32::INFINITY
            } else {
                self.bots.brains[&bot]
                    .plan
                    .last()
                    .map_or(distance, |p| flat(p.feet - at).length().max(distance))
            };
            let arrival_speed = handling
                .stopping_speed(gap.map_or(end, |g| end.min(g)).max(0.0))
                .max(2.0);
            let brake = signed_speed * travel_sign < -0.5
                || speed > corner_speed.min(arrival_speed)
                || gap.is_some_and(|g| g <= 0.0) && travel_sign > 0.0;
            input.forward = if toward.is_none() || hazard || waiting || brake {
                0.0
            } else {
                travel_sign
                    * (1.0 - heading_error.abs() * 0.3).clamp(0.25, crate::route::DRIVE_THROTTLE)
            };
            input.jump = input.forward == 0.0;
            // Driven to where its objective's step needs its feet (a flag
            // to take up): near enough, it pulls up and walks the rest. A
            // step that is itself the drive (the vehicle delivered) keeps it
            // at the controls.
            let brain = &self.bots.brains[&bot];
            let arrived = behaviour == Behaviour::Objective
                && brain.objective.drive(self, bot).is_none()
                && brain.goal.is_some_and(|g| {
                    matches!(g, Goal::Objective(_))
                        && flat(g.point(brain.home) - at).length() <= standoff + OBJECTIVE_WALK
                });
            let leave = arrived
                || carried.is_some_and(|(_, enemy, velocity)| {
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
                || matches!(
                    behaviour,
                    Behaviour::Return | Behaviour::Objective | Behaviour::Explore
                );
            // Headway is getting somewhere: a chassis rocking back and
            // forth against what blocks it keeps moving but goes nowhere.
            // Braking for an impassable route is also no headway; throttle
            // and contact impulses alone cannot renew a task.
            if waiting || !pursuing {
                brain.vehicle_headway.mark(at, tick);
            } else {
                let goal = brain
                    .goal
                    .map(|g| g.point(brain.home))
                    .or_else(|| toward.map(|delta| at + delta));
                brain.vehicle_headway.note(at, tick, |mark, at| {
                    // Backing away to recover, or being pushed sideways,
                    // does not renew a blocked journey by itself.
                    goal.is_some_and(|goal| {
                        flat(goal - at).length() + VEHICLE_PROGRESS <= flat(goal - mark).length()
                    })
                });
            }
            let stuck = brain.vehicle_headway.idle(tick);
            // It backs up from what holds it up ahead (an ally, a hull, a
            // drop) where the way behind is clear of each.
            let rear = if travel_sign > 0.0 {
                d.bounds_max[2]
            } else {
                -d.bounds_min[2]
            } * v.scale;
            let behind_clear = backing_up(stuck)
                && !self.bot_vehicle_path_blocked(
                    bot,
                    &v,
                    -travel,
                    0.0,
                    ALLY_ROOM,
                    (rear + ALLY_ROOM, radius),
                )
                && !self.bot_vehicle_edge(&v, at - travel * radius, -travel);
            if behind_clear {
                // This is opposite the requested travel, whose rear we just
                // checked, rather than unconditionally reverse transmission.
                // Straighten the wheels and stop forward momentum first.
                input.forward = if signed_speed * travel_sign > STOPPED_SPEED {
                    0.0
                } else {
                    -0.5 * travel_sign
                };
                input.jump = input.forward == 0.0;
                if looks {
                    input.head_yaw = 0.0;
                } else {
                    input.yaw = wrap(previous - v.mouse_steering[0].clamp(-0.04, 0.04));
                }
            }
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            if stuck >= VEHICLE_STALLED && stuck.is_multiple_of(120) {
                brain.plan.clear();
                brain.search = None;
                brain.settled = false;
                // A place it cannot drive to is looked for elsewhere.
                brain.explore.to = None;
            }
            if stuck == VEHICLE_STALLED {
                self.bots.reports.tell(Report::Stalled(bot));
            }
            if stuck >= VEHICLE_GIVE_UP {
                brain.next_interaction = tick + GIVE_UP_TICKS;
                brain.vehicle_headway.restart(tick);
                self.bot_leave_seat(bot, Leave::Stalled);
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
                self.bot_leave_seat(bot, Leave::WalkTheRest);
                return Ok(MoveInput {
                    yaw: self.bots.brains[&bot].yaw,
                    ..Default::default()
                });
            }
            if self.mounted(bot) == Some((vehicle, seat)) {
                self.bots.brains.get_mut(&bot).unwrap().driving = Some(BotDriving {
                    vehicle,
                    waypoint: next.map(|p| p.feet.to_array()),
                    throttle: input.forward,
                    brake: input.jump,
                    obstacle,
                    edge,
                    waiting,
                    recovering: backing_up(stuck),
                    idle_ticks: stuck,
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
            let driver_coming = self.bot_coming_aboard(bot, vehicle, tick);
            // Carried only, by a teammate driving away from where it goes.
            let goal = self.bots.brains[&bot]
                .goal
                .map(|g| g.point(self.bots.brains[&bot].home));
            let other_way = carried_only
                && needs_travel
                && goal.is_some_and(|g| self.bot_driver_goes_toward(&v, g, tick) == Some(false));
            input.forward = 0.0;
            input.right = 0.0;
            input.jump = false;
            if unusable_gun || needs_travel && !has_driver && !driver_coming || other_way {
                self.bots.brains.get_mut(&bot).unwrap().next_interaction = tick + GIVE_UP_TICKS;
                let why = if unusable_gun {
                    Leave::GunLost
                } else if other_way {
                    Leave::OtherWay
                } else {
                    Leave::NoDriver
                };
                self.bot_leave_seat(bot, why);
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
        // Nothing to do from the seat (it wanders: no enemy, task or place
        // to go) and no ally driving it: it gets out, looked at on a beat
        // so a moment's lull between choices is not taken for it.
        if behaviour == Behaviour::Wander
            && super::cadence::beat(bot, super::cadence::salt::DISMOUNT, tick, DISMOUNT_BEAT)
            && !riding_along
            // Carried to an enemy now in its band: it gets off to fight.
            || carried_only && behaviour == Behaviour::Fight
        {
            let why = if carried_only && behaviour == Behaviour::Fight {
                // Not straight back on board for the fight it got off for.
                self.bots.brains.get_mut(&bot).unwrap().next_interaction = tick + GIVE_UP_TICKS;
                Leave::GotOffToFight
            } else {
                Leave::NothingToDo
            };
            self.bot_leave_seat(bot, why);
        }
        Ok(input)
    }
}

/// Why a bot leaves its seat: each an outcome it observed, never a timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Leave {
    /// The hull is destroyed.
    Wrecked,
    /// A wheeled hull on its side or its roof.
    Overturned,
    /// A wheeled chassis whose stop could not be measured
    /// ([`crate::reach::Handling`]), so it cannot be driven safely.
    Undrivable,
    /// The next hit like the last would wreck it.
    BailOut,
    /// Its drive made no headway, backing up and replanning included.
    Stalled,
    /// The leave leg: a chassis that cannot hurt its quarry carried it
    /// there, and on foot it closes the rest.
    WalkTheRest,
    /// Carried to an enemy now in its fighting band.
    GotOffToFight,
    /// A seat that needs a driver to get anywhere, with none on its way.
    NoDriver,
    /// Carried only, by a teammate driving away from where it goes.
    OtherWay,
    /// A gun seat whose gun is gone.
    GunLost,
    /// Nothing to do from the seat: no enemy, task or place to go, and no
    /// ally driving it.
    NothingToDo,
}
impl Leave {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Wrecked => "wrecked",
            Self::Overturned => "overturned",
            Self::Undrivable => "undrivable",
            Self::BailOut => "bail_out",
            Self::Stalled => "stalled",
            Self::WalkTheRest => "walk_the_rest",
            Self::GotOffToFight => "got_off_to_fight",
            Self::NoDriver => "no_driver",
            Self::OtherWay => "other_way",
            Self::GunLost => "gun_lost",
            Self::NothingToDo => "nothing_to_do",
        }
    }
}

impl Session {
    /// Gets `bot` out of its seat for `why`, counted for the reports
    /// ([`Session::bot_seat_leaves`]).
    fn bot_leave_seat(&mut self, bot: OwnerId, why: Leave) {
        if self.vehicles.world.as_ref().is_some_and(|world| {
            world.has_clear_exit(&self.simulation.physics, bri_vehicles::OccupantId(bot))
        }) && self.dismount_vehicle(bot).is_ok()
        {
            *self.bots.leaves.entry(why.name()).or_default() += 1;
        }
    }
}

/// How much deeper than a body's planned drop (`nav::Body::drop`) the edge
/// check still finds floor on its route: the route planner samples floors
/// at cell centres, so a drop it plans can measure a little deeper from
/// where the body stands at the edge.
const ROUTE_DROP_SLACK: f32 = 0.5;
/// Slower than this a chassis is at rest: a stop for a drop is over.
const STOPPED_SPEED: f32 = 0.5;
/// How far a driven chassis must get from where it last made headway for
/// that to count as headway again.
const VEHICLE_PROGRESS: f32 = 3.0;
/// Room a driver leaves an ally on foot beyond where it would stop.
const ALLY_ROOM: f32 = 2.0;
/// The most a driver's predicted path turns between two of the hull
/// checks swept along it, radians.
const ARC_STEP: f32 = std::f32::consts::FRAC_PI_8;
/// Ticks without headway after which a driven chassis backs straight up for
/// a second, and plans again every second.
const VEHICLE_STALLED: u64 = 360;
fn backing_up(stuck: u64) -> bool {
    stuck > VEHICLE_STALLED && stuck < VEHICLE_STALLED + 120
}
/// Ticks without headway after which the driver gets out.
const VEHICLE_GIVE_UP: u64 = 720;

#[cfg(test)]
mod passage_tests {
    use super::*;

    fn pair(half_width: Option<f32>) -> (Session, [OwnerId; 2]) {
        let mut solids = vec![
            ColliderBuilder::cuboid(30.0, 0.5, 30.0)
                .translation(Vector::new(0.0, -0.5, 0.0))
                .user_data(u128::MAX),
        ];
        if let Some(half) = half_width {
            for x in [-half - 0.5, half + 0.5] {
                solids.push(
                    ColliderBuilder::cuboid(0.5, 4.0, 20.0)
                        .translation(Vector::new(x, 4.0, 0.0))
                        .user_data(u128::MAX),
                );
            }
        }
        let sim = crate::simulation::Simulation::new(
            bri_world::World::new("Passage".into(), "fixture".into(), vec![[1.0; 4]]),
            crate::testing::definitions(),
            solids,
        )
        .unwrap();
        let mut s = Session::new(sim);
        let a = s.join("First".into(), Vec3::Y * 0.05, true).unwrap();
        let gap = s.peers[&a].player.tuning().width + 0.2;
        let b = s
            .join("Second".into(), Vec3::new(0.0, 0.05, gap), false)
            .unwrap();
        super::super::harm::one_game(&mut s, a, b);
        let gid = s.game_of(a).unwrap();
        let (teams, _) = s
            .minigames
            .set_teams(
                gid,
                vec![bri_minigames::TeamSpec {
                    id: None,
                    name: "Allies".into(),
                    color: 0,
                    player_type: None,
                }],
                false,
                false,
            )
            .unwrap();
        for id in [a, b] {
            s.minigames
                .assign_team(s.peers[&id].combat.player, Some(teams[0]))
                .unwrap();
            let at = Vec3::from(s.peers[&id].player.state().feet);
            let mut brain = Brain::new(None, BotKind::default(), at, id, 0);
            let way = if id == a { Vec3::Z } else { Vec3::NEG_Z };
            brain.plan.push(Waypoint::walk(at + way * 10.0));
            s.bots.brains.insert(id, brain);
        }
        s.bots.relations.lock().unwrap().clear();
        assert!(s.bot_allies(a, b));
        (s, [a, b])
    }

    #[test]
    fn oncoming_allies_yield_along_a_narrow_passage_instead_of_into_its_walls() {
        let (s, [a, b]) = pair(Some(0.96));
        let first = s.bot_walk_direction(a, Vec3::Z, None, None);
        let second = s.bot_walk_direction(b, Vec3::NEG_Z, None, None);
        assert!(
            first.z > 0.9 && second.z > 0.9,
            "one advances and the other yields backward along the passage: {first}, {second}"
        );
        assert!(
            first.x.abs() < 0.01 && second.x.abs() < 0.01,
            "neither requests a pass into a wall: {first}, {second}"
        );
    }

    #[test]
    fn an_open_space_still_allows_oncoming_allies_to_pass_laterally() {
        let (s, [a, b]) = pair(None);
        let first = s.bot_walk_direction(a, Vec3::Z, None, None);
        let second = s.bot_walk_direction(b, Vec3::NEG_Z, None, None);
        assert!(first.x.abs() > 0.5 && first.z > 0.0);
        assert!(second.x.abs() > 0.5 && second.z < 0.0);
    }

    #[test]
    fn nearby_walls_do_not_force_yielding_when_both_bodies_can_pass() {
        let (s, [a, b]) = pair(Some(1.7));
        let first = s.bot_walk_direction(a, Vec3::Z, None, None);
        let second = s.bot_walk_direction(b, Vec3::NEG_Z, None, None);
        assert!(first.x.abs() > 0.5 && first.z > 0.0);
        assert!(second.x.abs() > 0.5 && second.z < 0.0);
    }

    #[test]
    fn a_file_follows_a_moving_ally_and_yields_to_a_stationary_one() {
        let (mut s, [a, b]) = pair(Some(0.96));
        s.bots.brains.get_mut(&b).unwrap().plan.clear();
        assert_eq!(
            s.bot_walk_direction(a, Vec3::Z, None, None),
            Vec3::NEG_Z,
            "cannot assume an idle ally will reverse"
        );
        let speed = s.peers[&a].player.tuning().forward * 0.5;
        s.peers
            .get_mut(&b)
            .unwrap()
            .player
            .set_motion(Vec3::Z * speed, true);
        assert!(
            (s.bot_walk_direction(a, Vec3::Z, None, None) - Vec3::Z * 0.5).length() < 0.001,
            "retain same-direction file pacing"
        );
    }

    #[test]
    fn an_ally_on_another_floor_does_not_block_a_passage() {
        let (mut s, [a, b]) = pair(Some(0.96));
        let mut at = Vec3::from(s.peers[&b].player.state().feet);
        at.y += 10.0;
        s.peers
            .get_mut(&b)
            .unwrap()
            .player
            .teleport(&mut s.simulation.physics, at, 0.0)
            .unwrap();
        assert_eq!(s.bot_walk_direction(a, Vec3::Z, None, None), Vec3::Z);
    }
}

#[cfg(test)]
mod driving_collision_tests {
    use super::*;

    fn tapered_chassis(obstacles: Vec<ColliderBuilder>) -> (Session, OwnerId, VehicleSnapshot) {
        let sim = crate::simulation::Simulation::new(
            bri_world::World::new("Tapered collision".into(), "fixture".into(), vec![[1.0; 4]]),
            crate::testing::definitions(),
            obstacles,
        )
        .unwrap();
        let mut s = Session::new(sim);
        let pack = bri_vehicles::testing::pack_with(|d| {
            if d.id == bri_vehicles::testing::CAR {
                d.bounds_min = [-2.0, 0.0, -4.0];
                d.bounds_max = [2.0, 2.0, 4.0];
                d.collision_hulls = vec![vec![
                    [-0.4, 0.0, -4.0],
                    [0.4, 0.0, -4.0],
                    [-0.4, 2.0, -4.0],
                    [0.4, 2.0, -4.0],
                    [-2.0, 0.0, 4.0],
                    [2.0, 0.0, 4.0],
                    [-2.0, 2.0, 4.0],
                    [2.0, 2.0, 4.0],
                ]];
            }
        });
        s.set_vehicle_pack(pack, vec![]).unwrap();
        let bot = s
            .join("Driver".into(), Vec3::new(20.0, 3.0, 0.0), true)
            .unwrap();
        s.bots.brains.insert(
            bot,
            Brain::new(None, BotKind::default(), Vec3::ZERO, bot, 0),
        );
        let world = s.vehicles.world.as_mut().unwrap();
        world
            .spawn(
                &mut s.simulation.physics,
                bri_vehicles::Spawn {
                    id: VehicleId(1),
                    owner: bri_vehicles::OwnerId(bot),
                    definition: bri_vehicles::testing::CAR.into(),
                    transform: Default::default(),
                    spawn_id: None,
                    respawn_ticks: None,
                    scale: 1.0,
                },
            )
            .unwrap();
        for h in world.colliders_of(&s.simulation.physics, VehicleId(1)) {
            s.simulation.physics.colliders[h].user_data =
                super::super::super::vehicles::VEHICLE_TAG | 1;
        }
        s.simulation.physics.step();
        let v = s
            .vehicles
            .world
            .as_ref()
            .unwrap()
            .vehicle_snapshot(&s.simulation.physics, VehicleId(1))
            .unwrap();
        (s, bot, v)
    }

    #[test]
    fn empty_visual_corners_do_not_block_a_tapered_driving_hull() {
        let pillar = ColliderBuilder::cuboid(0.2, 1.0, 0.2).translation(Vec3::new(1.6, 1.0, -3.0));
        let (s, bot, v) = tapered_chassis(vec![pillar]);
        assert!(s.bot_vehicle_clear(bot, &v, Vec3::NEG_Z, 2.0));
        assert!(
            !s.bot_vehicle_clear(bot, &v, Vec3::X, 2.0),
            "the real broad side still hits the pillar"
        );
    }

    #[test]
    fn real_obstacles_and_support_contacts_keep_distinct_meanings() {
        let floor = ColliderBuilder::cuboid(30.0, 0.5, 30.0).translation(Vec3::new(0.0, -0.5, 0.0));
        let wall = ColliderBuilder::cuboid(10.0, 3.0, 0.2).translation(Vec3::new(0.0, 3.0, -6.0));
        let (s, bot, v) = tapered_chassis(vec![floor, wall]);
        assert!(
            s.bot_vehicle_clear(bot, &v, Vec3::Z, 2.0),
            "may depart from its supporting floor"
        );
        assert!(
            !s.bot_vehicle_clear(bot, &v, Vec3::NEG_Z, 4.0),
            "must brake for the actual wall"
        );
    }

    #[test]
    fn crew_wait_has_one_deadline_even_when_claimants_change() {
        let (mut s, bot, _) = tapered_chassis(vec![]);
        s.bots.brains.get_mut(&bot).unwrap().vehicle_since = Some((1, 100));
        assert!(s.bot_assembling_crew(bot, 100 + DISMOUNT_BEAT - 1));
        assert!(!s.bot_assembling_crew(bot, 100 + DISMOUNT_BEAT));
        assert!(!s.bot_assembling_crew(bot, 10_000));
    }

    #[test]
    fn a_rooftop_enemy_is_not_a_runover_target_even_inside_the_guns_near_edge() {
        let floor = ColliderBuilder::cuboid(30.0, 0.5, 30.0).translation(Vec3::new(0.0, -0.5, 0.0));
        let roof = ColliderBuilder::cuboid(2.0, 0.5, 2.0).translation(Vec3::new(8.0, 9.5, 0.0));
        let (mut s, bot, v) = tapered_chassis(vec![floor, roof]);
        let high = s
            .join("On roof".into(), Vec3::new(8.0, 10.05, 0.0), false)
            .unwrap();
        let low = s
            .join("On street".into(), Vec3::new(8.0, 0.05, 0.0), false)
            .unwrap();
        super::super::harm::one_game(&mut s, bot, high);
        // The second fixture target joins that same game's authoritative rules.
        let game = s.game_of(bot).unwrap();
        s.command(
            low,
            1,
            super::super::super::Command::MiniGame(super::super::super::MiniGameRequest::Join {
                game: game.0,
            }),
        )
        .unwrap();
        let peer = s.peers.get_mut(&low).unwrap();
        peer.player
            .teleport(&mut s.simulation.physics, Vec3::new(8.0, 0.05, 0.0), 0.0)
            .unwrap();
        peer.combat.shot_once = true;
        s.peers
            .get_mut(&bot)
            .unwrap()
            .player
            .teleport(
                &mut s.simulation.physics,
                Vec3::from(v.seats[0].transform.position),
                0.0,
            )
            .unwrap();
        s.board_vehicle(bot, 1, 0).unwrap();
        assert!(
            s.bot_runs_over_near(bot, low, 12.0),
            "grounded enemies still permit runover"
        );
        assert!(
            !s.bot_runs_over_near(bot, high, 12.0),
            "a nearby roof does not make its occupant reachable by the chassis"
        );
    }

    #[test]
    fn an_ally_above_the_chassis_is_not_a_phantom_traffic_obstacle() {
        let floor = ColliderBuilder::cuboid(30.0, 0.5, 30.0).translation(Vec3::new(0.0, -0.5, 0.0));
        let roof = ColliderBuilder::cuboid(2.0, 0.5, 2.0).translation(Vec3::new(8.0, 9.5, 0.0));
        let (mut s, bot, v) = tapered_chassis(vec![floor, roof]);
        let ally = s
            .join("Ally".into(), Vec3::new(8.0, 10.05, 0.0), false)
            .unwrap();
        super::super::harm::one_game(&mut s, bot, ally);
        let game = s.game_of(bot).unwrap();
        let (teams, _) = s
            .minigames
            .set_teams(
                game,
                vec![bri_minigames::TeamSpec {
                    id: None,
                    name: "Crew".into(),
                    color: 0,
                    player_type: None,
                }],
                false,
                false,
            )
            .unwrap();
        for owner in [bot, ally] {
            s.minigames
                .assign_team(s.peers[&owner].combat.player, Some(teams[0]))
                .unwrap();
        }
        s.bots.relations.lock().unwrap().clear();
        assert!(s.bot_allies(bot, ally));
        assert!(
            !s.bot_ally_corridor(bot, &v, Vec3::ZERO, Vec3::X, 12.0, 2.0),
            "a roof above the road does not close the road below"
        );
        s.peers
            .get_mut(&ally)
            .unwrap()
            .player
            .teleport(&mut s.simulation.physics, Vec3::new(8.0, 0.05, 0.0), 0.0)
            .unwrap();
        assert!(
            s.bot_ally_corridor(bot, &v, Vec3::ZERO, Vec3::X, 12.0, 2.0),
            "the same ally on the street still requires braking"
        );
    }

    #[test]
    fn a_changing_grid_waypoint_does_not_reverse_a_walk_around_the_same_chassis() {
        let (mut s, bot, _) = tapered_chassis(vec![]);
        s.peers
            .get_mut(&bot)
            .unwrap()
            .player
            .teleport(&mut s.simulation.physics, Vec3::new(-2.7, 0.05, 0.0), 0.0)
            .unwrap();
        s.observe_bot_objects();
        let goal = Some(Vec3::X * 8.0);
        let first = s.bot_vehicle_detour(bot, Vec3::new(0.8, 0.0, 0.6), None, goal);
        let second = s.bot_vehicle_detour(bot, Vec3::new(0.8, 0.0, -0.6), None, goal);
        assert!(first.z.abs() > 0.5 && second.z.abs() > 0.5);
        assert!(
            first.z * second.z > 0.0,
            "the grid's next point changed; the obstacle and chosen way around it did not: {first:?} -> {second:?}"
        );
        let away = s.bot_vehicle_detour(bot, Vec3::NEG_X, None, Some(Vec3::NEG_X * 12.0));
        assert_eq!(
            away,
            Vec3::NEG_X,
            "a clear new direction is not held to the old detour"
        );
        assert!(s.bots.brains[&bot].vehicle_detour.is_none());
    }

    #[test]
    fn drive_navigation_starts_on_the_actual_spawn_plate_not_extended_suspension() {
        let floor = ColliderBuilder::cuboid(30.0, 0.5, 30.0).translation(Vec3::new(0.0, -0.1, 0.0));
        let plate = ColliderBuilder::cuboid(2.0, 0.1, 2.0).translation(Vec3::new(0.0, 0.5, 0.0));
        let (mut s, bot, _) = tapered_chassis(vec![floor, plate]);
        for _ in 0..240 {
            s.vehicles
                .world
                .as_mut()
                .unwrap()
                .pre_step(&mut s.simulation.physics, &[])
                .unwrap();
            s.simulation.physics.step();
            s.vehicles
                .world
                .as_mut()
                .unwrap()
                .post_step(&mut s.simulation.physics)
                .unwrap();
        }
        let v = s
            .vehicles
            .world
            .as_ref()
            .unwrap()
            .vehicle_snapshot(&s.simulation.physics, VehicleId(1))
            .unwrap();
        let seat = Vec3::from(v.seats[0].transform.position);
        s.peers
            .get_mut(&bot)
            .unwrap()
            .player
            .teleport(&mut s.simulation.physics, seat, 0.0)
            .unwrap();
        s.board_vehicle(bot, 1, 0).unwrap();
        let (feet, _) = s.bot_vehicle_body(bot).unwrap();
        assert!(
            (feet.y - 0.6).abs() < 0.01,
            "the grid starts at the supporting plate: {feet:?}"
        );
    }
}
