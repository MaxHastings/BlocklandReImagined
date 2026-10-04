//! Grounded physical methods for an exact creator-owned object-entry goal.
//!
//! This provider supplies choices and ordinary control requests. It neither
//! dispatches an event nor predicts that reaching a point completed the rule.
//! The objective owner observes the canonical captured-object input and effects.
use super::claims::Resource;
use super::interactions::push_approach;
use super::*;
use bri_vehicles::{Definition, Family, VehicleId, VehicleSnapshot};
use bri_weapons::BotManipulation;

// The caller charges every resulting choice against the existing cumulative
// action/model budget before cloning rows. The discovery envelope is the
// kind's `objective_radius`.
const OBJECTS: usize = 8;

fn discovery_radius(session: &Session, bot: OwnerId) -> f32 {
    session
        .bots
        .brains
        .get(&bot)
        .map_or(BotKind::default().objective_radius, |b| {
            b.kind.objective_radius
        })
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ObjectStamp {
    pub vehicle: u64,
    pub spawner: BrickId,
    pub definition: String,
    pub scale: f32,
    spawner_name: Option<String>,
    owner: OwnerId,
    mass: f32,
    bounds: ([f32; 3], [f32; 3]),
    speed: f32,
    engine_force: f32,
    control_seat: Option<usize>,
    family: Family,
    flies: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Goal {
    pub source: BrickId,
    pub object: ObjectStamp,
    owner: OwnerId,
    pub bounds: (Vec3, Vec3),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Method {
    Push,
    Hammer {
        slot: usize,
        image: String,
    },
    Hold {
        slot: usize,
        image: String,
        descriptor: BotManipulation,
    },
    Drive {
        seat: u8,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Choice {
    pub goal: Goal,
    pub method: Method,
    pub cost: u32,
    rearm: Option<Vec3>,
    observed_distance: f32,
    progressed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Rejection {
    Missing,
    Changed,
    Forbidden,
    Occupied,
    Unsupported,
    Budget,
}

impl Rejection {
    pub(super) fn diagnostic(self) -> &'static str {
        match self {
            Self::Missing => "objective object missing",
            Self::Changed => "objective object or region changed",
            Self::Forbidden => "objective object permission changed",
            Self::Occupied => "objective object occupied",
            Self::Unsupported => "no grounded physical method",
            Self::Budget => "physical objective candidate budget",
        }
    }
}

/// Normal controls only. Root translates boarding through the existing oriented
/// seat approach/reach adapter and reserves `resource` through shared claims.
#[derive(Clone, Copy, Debug)]
pub(super) struct Directive {
    pub point: Vec3,
    pub aim: Vec3,
    pub resource: Resource,
    pub equip: Option<usize>,
    pub trigger: bool,
    pub board: Option<(u64, u8)>,
    pub held: Option<ObjectRef>,
    pub drive: Option<(u64, u8)>,
    pub physical_progress: bool,
}

fn stamp(session: &Session, v: &VehicleSnapshot, d: &Definition) -> Option<ObjectStamp> {
    Some(ObjectStamp {
        vehicle: v.id.0,
        spawner: session.vehicle_spawn_brick(v.id)?,
        definition: v.definition.clone(),
        scale: v.scale,
        spawner_name: session
            .simulation
            .state()
            .bricks
            .get(&session.vehicle_spawn_brick(v.id)?)?
            .name
            .clone(),
        owner: v.owner.0,
        mass: d.mass,
        bounds: (d.bounds_min, d.bounds_max),
        speed: d.max_speed,
        engine_force: d.engine_force,
        control_seat: d.control_seat(),
        family: d.family,
        flies: d.wheeled_flight.is_some(),
    })
}

fn region(session: &Session, source: BrickId) -> Option<(OwnerId, (Vec3, Vec3))> {
    let b = session.simulation.state().bricks.get(&source)?;
    let bounds = bri_world::regions::bounds(b.rule_region, session.simulation.brick_box(source)?);
    (bounds.0.is_finite() && bounds.1.is_finite()).then_some((b.owner, bounds))
}

fn inside(point: Vec3, bounds: (Vec3, Vec3)) -> bool {
    point.cmpge(bounds.0).all() && point.cmple(bounds.1).all()
}

/// Native ObjectEnter observes the transform origin, not hull overlap. Clear
/// the region horizontally before requesting another entry; the hull margin
/// gives the same ordinary controls room to turn back without a grazing edge.
fn exit_point(point: Vec3, bounds: (Vec3, Vec3), margin: f32) -> Option<Vec3> {
    if !point.is_finite() || !margin.is_finite() || margin <= 0.0 {
        return None;
    }
    [
        Vec3::new(bounds.0.x - margin, point.y, point.z),
        Vec3::new(bounds.1.x + margin, point.y, point.z),
        Vec3::new(point.x, point.y, bounds.0.z - margin),
        Vec3::new(point.x, point.y, bounds.1.z + margin),
    ]
    .into_iter()
    .filter(|p| p.is_finite() && !inside(*p, bounds))
    .min_by(|a, b| {
        point
            .distance_squared(*a)
            .total_cmp(&point.distance_squared(*b))
    })
}

/// Reach the body's delivery-rear standoff without walking through its
/// moving hull. Reuse the ordinary physical approach arc before acquiring;
/// the native hold then moves the body ahead of its holder toward delivery.
fn hold_acquisition_approach(
    feet: Vec3,
    centre: Vec3,
    toward: Vec3,
    distance: f32,
    width: f32,
) -> Option<(Vec3, bool)> {
    let toward = flat(toward).try_normalize()?;
    let final_point = centre - toward * distance;
    let ready = flat(final_point - feet).length() <= 0.5;
    let approach = push_approach(feet, centre, toward, distance, width)?;
    let mut point = if ready || approach.pushing {
        final_point
    } else {
        approach.point
    };
    point.y = feet.y;
    Some((point, ready))
}

fn effort(distance: f32, speed: f32, setup: f32) -> u32 {
    // A ranking estimate, not a predicted physics result. Observed displacement,
    // native control readiness and canonical admission decide progress/failure.
    (1.0 + (distance / speed.max(0.1) + setup) * 120.0).clamp(1.0, 100000.0) as u32
}

/// One bounded physical observation set per model build, shared across all
/// causal regions. Charge visits before filtering so distant/decorative bodies
/// cannot turn each source into another whole-world scan.
pub(super) struct Discovery {
    bodies: Vec<VehicleSnapshot>,
}

pub(super) fn discover(
    session: &Session,
    bot: OwnerId,
    budget: &mut super::objectives::GroundingBudget,
) -> Result<Discovery, Rejection> {
    let peer = session.peers.get(&bot).ok_or(Rejection::Missing)?;
    let feet = Vec3::from(peer.player.state().feet);
    let radius = discovery_radius(session, bot);
    let mut bodies = Vec::new();
    for v in &session.bots.objects {
        budget.reserve(1, 0, 0).map_err(|_| Rejection::Budget)?;
        if v.destroyed || feet.distance(Vec3::from(v.transform.position)) > radius {
            continue;
        }
        if bodies.len() >= OBJECTS {
            return Err(Rejection::Budget);
        }
        budget
            .reserve(
                0,
                v.seats.len(),
                v.definition.len() + v.seats.iter().map(|s| s.pose.len()).sum::<usize>(),
            )
            .map_err(|_| Rejection::Budget)?;
        bodies.push(v.clone());
    }
    Ok(Discovery { bodies })
}

/// Exact-object candidates are filtered by the caller's canonical grouped guards
/// (including named spawner/kind) before insertion into the shared GOAP model.
pub(super) fn candidates(
    session: &Session,
    bot: OwnerId,
    source: BrickId,
    discovery: &Discovery,
    budget: &mut super::objectives::GroundingBudget,
) -> Result<Vec<Choice>, Rejection> {
    let (owner, bounds) = region(session, source).ok_or(Rejection::Missing)?;
    let peer = session.peers.get(&bot).ok_or(Rejection::Missing)?;
    let feet = Vec3::from(peer.player.state().feet);
    let world = session
        .vehicles
        .world
        .as_ref()
        .ok_or(Rejection::Unsupported)?;
    let destination = (bounds.0 + bounds.1) * 0.5;
    let radius = discovery_radius(session, bot);
    let mut choices = Vec::new();
    for v in &discovery.bodies {
        if v.destroyed || feet.distance(Vec3::from(v.transform.position)) > radius {
            continue;
        }
        let Some(d) = world.definition(&v.definition) else {
            continue;
        };
        budget
            .reserve(
                0,
                1,
                v.definition.len()
                    + session
                        .vehicle_spawn_brick(v.id)
                        .and_then(|id| session.simulation.state().bricks.get(&id))
                        .and_then(|b| b.name.as_ref())
                        .map_or(0, String::len),
            )
            .map_err(|_| Rejection::Budget)?;
        let Some(object) = stamp(session, v, d) else {
            continue;
        };
        if session
            .simulation
            .state()
            .bricks
            .get(&object.spawner)
            .is_none_or(|b| b.owner != owner)
        {
            continue;
        }
        let target = ObjectRef::Vehicle(v.id.0);
        if session.object_held(target) && session.held_by(bot) != Some(target) {
            continue;
        }
        // Grab/contact authority deliberately excludes one's own ridden body.
        // A ground seat instead uses ordinary ride permission below; boarding
        // must not invalidate the very Drive method that requested the seat.
        let movable = session.may_move(bot, target);
        let at = Vec3::from(v.transform.position);
        let rearm = if inside(at, bounds) {
            let margin =
                (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)).length() * v.scale * 0.5
                    + 0.25;
            let Some(exit) = exit_point(at, bounds, margin) else {
                continue;
            };
            Some(exit)
        } else {
            None
        };
        let travel = rearm.map_or_else(
            || flat(destination - at).length(),
            |exit| flat(exit - at).length() + flat(destination - exit).length(),
        );
        let initial_distance = at.distance(rearm.unwrap_or(destination));
        let approach = flat(at - feet).length();
        let goal = Goal {
            source,
            object,
            owner,
            bounds,
        };
        // Walking contact cannot deliberately raise a loose body to an elevated
        // sensor. It stays eligible for ground-height regions; terrain and wall
        // reachability remain the existing navigation executor's responsibility.
        if movable
            && (d.seats.is_empty()
                || session.can_ride(bot, v.owner.0) && v.seats.iter().all(|s| s.occupant.is_none()))
            && bounds.0.y <= at.y + 0.5
            && bounds.1.y >= at.y - 0.5
        {
            let speed = if session.can_ride(bot, v.owner.0) {
                4.0 / v.scale.max(0.1)
            } else {
                4.0 / (1.0 + d.mass.max(0.0) / combat::PLAYER_MASS)
            };
            budget
                .reserve(
                    0,
                    1,
                    goal.object.definition.len()
                        + goal.object.spawner_name.as_ref().map_or(0, String::len),
                )
                .map_err(|_| Rejection::Budget)?;
            choices.push(Choice {
                goal: goal.clone(),
                method: Method::Push,
                cost: effort(approach, 4.0, 0.25) + effort(travel, speed, 0.0),
                rearm,
                observed_distance: initial_distance,
                progressed: false,
            });
        }
        if movable
            && v.seats.iter().all(|s| s.occupant.is_none())
            && bounds.0.y <= at.y + 0.5
            && bounds.1.y >= at.y - 0.5
            && session.hammer_vehicle_allowed(bot, v.id.0)
            && let Some(actor) = session.weapons.actor(ActorId(bot))
        {
            for (slot, image) in actor
                .inventory
                .iter()
                .enumerate()
                .take(super::super::inventory::TOOL_SLOTS)
                .filter_map(|(slot, item)| {
                    item.as_ref()
                        .and_then(|id| session.weapons.pack.items.get(id))
                        .and_then(|item| session.weapons.pack.images.get(&item.image))
                        .map(|i| (slot, i))
                })
                .filter(|(_, image)| super::super::tools::native_hammer(image))
            {
                budget
                    .reserve(0, 1, goal.object.definition.len() + image.id.len())
                    .map_err(|_| Rejection::Budget)?;
                choices.push(Choice {
                    goal: goal.clone(),
                    method: Method::Hammer {
                        slot,
                        image: image.id.clone(),
                    },
                    cost: effort(approach, 4.0, 0.4) + effort(travel, 5.0, 0.0),
                    rearm,
                    observed_distance: initial_distance,
                    progressed: false,
                });
            }
        }
        if movable
            && v.seats.iter().all(|s| s.occupant.is_none())
            && let Some(actor) = session.weapons.actor(ActorId(bot))
        {
            for (slot, item) in actor
                .inventory
                .iter()
                .enumerate()
                .take(super::super::inventory::TOOL_SLOTS)
            {
                let Some(image) = item
                    .as_ref()
                    .and_then(|i| session.weapons.pack.items.get(i))
                    .and_then(|i| session.weapons.pack.images.get(&i.image))
                else {
                    continue;
                };
                let Some(
                    descriptor @ BotManipulation::Hold {
                        near, reach, force, ..
                    },
                ) = image.bot.and_then(|b| b.manipulation)
                else {
                    continue;
                };
                if ![near, reach, force].into_iter().all(f32::is_finite)
                    || near <= 0.0
                    || reach < near
                    || force <= 0.0
                    || !session.bot_hold_can_lift(target, force)
                    || (destination.y - peer.player.eye().y).abs() >= reach
                {
                    continue;
                }
                let speed = (force / d.mass.max(1.0)).sqrt().clamp(0.1, 8.0);
                budget
                    .reserve(
                        0,
                        1,
                        goal.object.definition.len()
                            + goal.object.spawner_name.as_ref().map_or(0, String::len)
                            + image.id.len(),
                    )
                    .map_err(|_| Rejection::Budget)?;
                choices.push(Choice {
                    goal: goal.clone(),
                    method: Method::Hold {
                        slot,
                        image: image.id.clone(),
                        descriptor,
                    },
                    cost: effort(approach, 4.0, 0.75)
                        + effort(travel.hypot(destination.y - at.y), speed, 0.5),
                    rearm,
                    observed_distance: initial_distance,
                    progressed: false,
                });
            }
        }
        if d.family == Family::Wheeled
            && d.wheeled_flight.is_none()
            && d.max_speed > 0.0
            && d.engine_force > 0.0
            && session.can_ride(bot, v.owner.0)
            && session
                .archetypes
                .resolve(peer.player.state().archetype)
                .can_ride
            && let Some(seat) = d.control_seat().filter(|s| *s <= u8::MAX as usize)
            && world
                .seat_occupant(v.id, seat)
                .is_none_or(|o| o.owner.0 == bot)
            && v.seats.iter().all(|s| {
                s.occupant
                    .is_none_or(|o| session.bot_allies(bot, o.owner.0))
            })
        {
            budget
                .reserve(
                    0,
                    1,
                    goal.object.definition.len()
                        + goal.object.spawner_name.as_ref().map_or(0, String::len),
                )
                .map_err(|_| Rejection::Budget)?;
            choices.push(Choice {
                goal,
                method: Method::Drive { seat: seat as u8 },
                cost: effort(approach, 4.0, 1.0) + effort(travel, d.max_speed.min(12.0), 0.5),
                rearm,
                observed_distance: initial_distance,
                progressed: false,
            });
        }
    }
    Ok(choices)
}

impl Choice {
    pub(super) fn rearming(&self) -> bool {
        self.rearm.is_some()
    }
    pub(super) fn advance(&mut self, session: &Session, bot: OwnerId) {
        self.progressed = false;
        let object = ObjectRef::Vehicle(self.goal.object.vehicle);
        let Some(v) = session.vehicles.world.as_ref().and_then(|w| {
            w.vehicle_snapshot(
                &session.simulation.physics,
                VehicleId(self.goal.object.vehicle),
            )
        }) else {
            return;
        };
        let at = Vec3::from(v.transform.position);
        let destination = (self.goal.bounds.0 + self.goal.bounds.1) * 0.5;
        if self.rearm.is_some() && !inside(at, self.goal.bounds) {
            self.rearm = None;
            self.observed_distance = at.distance(destination);
            return;
        }
        let distance = at.distance(self.rearm.unwrap_or(destination));
        self.progressed = distance.is_finite()
            && distance < self.observed_distance - 0.25
            && session.mover_credit(object) == Some(bot);
        if self.progressed {
            self.observed_distance = distance;
        }
    }
    /// Native hand activation is a control, not an invented body force. Only
    /// click from the delivery side, after actual aim reaches this same body.
    pub(super) fn execute(&self, session: &mut Session, bot: OwnerId) -> Result<&'static str> {
        if !matches!(self.method, Method::Push) {
            return Ok("approach");
        }
        self.directive(session, bot)
            .map_err(|r| anyhow::anyhow!(r.diagnostic()))?;
        let peer = &session.peers[&bot];
        let world = session.vehicles.world.as_ref().unwrap();
        let v = world
            .vehicle_snapshot(
                &session.simulation.physics,
                VehicleId(self.goal.object.vehicle),
            )
            .unwrap();
        if v.seats.iter().any(|s| s.occupant.is_some())
            || Vec3::from(v.velocity).length() > 2.0
            || !session.can_ride(bot, v.owner.0)
        {
            return Ok("contact");
        }
        let centre = session.object_centre(ObjectRef::Vehicle(v.id.0)).unwrap();
        let destination = self
            .rearm
            .unwrap_or((self.goal.bounds.0 + self.goal.bounds.1) * 0.5);
        let d = world.definition(&v.definition).unwrap();
        let width = peer.player.tuning().width;
        let radius = (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)).length() * v.scale * 0.5
            + width * 0.5
            + 0.15;
        if !push_approach(
            Vec3::from(peer.player.state().feet),
            centre,
            destination - centre,
            radius,
            width,
        )
        .is_some_and(|a| a.pushing)
        {
            return Ok("approach");
        }
        let state = peer.player.state();
        let direction = Vec3::new(
            state.yaw.sin() * state.pitch.cos(),
            state.pitch.sin(),
            -state.yaw.cos() * state.pitch.cos(),
        );
        let eye = peer.player.eye();
        let brick = session
            .simulation
            .target_through(eye, direction, 5.0)?
            .and_then(|(hit, _)| hit.brick.map(|_| hit.distance));
        if session
            .vehicle_click_target(bot, eye, direction, brick)
            .map(|x| x.0.0)
            != Some(v.id.0)
            || peer
                .last_activate
                .is_some_and(|at| session.simulation.state().tick.saturating_sub(at) < 30)
        {
            return Ok("contact");
        }
        if session
            .weapons
            .actor(ActorId(bot))
            .is_some_and(|a| a.selected.is_some())
        {
            session.abort_bot_hand_charge(bot)?;
            session.equip_tool(bot, None)?;
            return Ok("preparing");
        }
        let sequence = session.peers[&bot].last_sequence.saturating_add(1);
        session.command(bot, sequence, Command::Activate)?;
        let sequence = session.peers[&bot].last_sequence.saturating_add(1);
        session.command(bot, sequence, Command::ActivateRelease)?;
        session.bots.brains.get_mut(&bot).unwrap().fire_down = false;
        Ok("native click")
    }
    pub(super) fn progressed(&self) -> bool {
        self.progressed
    }
    pub(super) fn resource(&self) -> Resource {
        match self.method {
            Method::Drive { seat } => Resource::Seat {
                vehicle: self.goal.object.vehicle,
                seat,
            },
            _ => Resource::Body {
                vehicle: self.goal.object.vehicle,
            },
        }
    }

    pub(super) fn key(&self) -> String {
        let method = match &self.method {
            Method::Push => "contact".into(),
            Method::Hammer { slot, image } => format!("hammer/{slot}/{image}"),
            Method::Hold { slot, image, .. } => format!("hold/{slot}/{image}"),
            Method::Drive { seat } => format!("drive/{seat}"),
        };
        format!(
            "{}/onObjectEnter/{}/{method}",
            self.goal.source, self.goal.object.vehicle
        )
    }

    /// Every use checks actual object identity, geometry, inventory, permission
    /// and occupancy. Context/program/round/team/delay checks stay in objectives.
    pub(super) fn directive(
        &self,
        session: &Session,
        bot: OwnerId,
    ) -> Result<Directive, Rejection> {
        let peer = session.peers.get(&bot).ok_or(Rejection::Missing)?;
        let world = session.vehicles.world.as_ref().ok_or(Rejection::Missing)?;
        let v = world
            .vehicle_snapshot(
                &session.simulation.physics,
                VehicleId(self.goal.object.vehicle),
            )
            .filter(|v| !v.destroyed)
            .ok_or(Rejection::Missing)?;
        let d = world.definition(&v.definition).ok_or(Rejection::Missing)?;
        if stamp(session, &v, d).as_ref() != Some(&self.goal.object)
            || region(session, self.goal.source) != Some((self.goal.owner, self.goal.bounds))
        {
            return Err(Rejection::Changed);
        }
        let object = ObjectRef::Vehicle(v.id.0);
        if !matches!(self.method, Method::Drive { .. }) && !session.may_move(bot, object) {
            return Err(Rejection::Forbidden);
        }
        if session.object_held(object) && session.held_by(bot) != Some(object) {
            return Err(Rejection::Occupied);
        }
        let feet = Vec3::from(peer.player.state().feet);
        let at = Vec3::from(v.transform.position);
        let mut centre = session.object_centre(object).ok_or(Rejection::Missing)?;
        // Contested by an opponent: meet the body where it is heading.
        if matches!(self.method, Method::Push | Method::Hammer { .. }) {
            centre += super::contest::lead(session, bot, object, feet, centre, v.velocity.into());
        }
        let destination = self
            .rearm
            .unwrap_or((self.goal.bounds.0 + self.goal.bounds.1) * 0.5);
        let toward = flat(destination - at).normalize_or(Vec3::NEG_Z);
        let mut out = Directive {
            point: feet,
            aim: centre,
            resource: self.resource(),
            equip: None,
            trigger: false,
            board: None,
            held: None,
            drive: None,
            physical_progress: self.progressed,
        };
        match &self.method {
            Method::Push | Method::Hammer { .. } => {
                let extents = (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)) * v.scale * 0.5;
                let width = peer.player.tuning().width;
                let radius = extents.length() + width * 0.5 + 0.15;
                let approach = push_approach(feet, centre, toward, radius, width)
                    .ok_or(Rejection::Unsupported)?;
                out.point = approach.point;
                if let Method::Hammer { slot, image } = &self.method {
                    let live = session
                        .weapons
                        .actor(ActorId(bot))
                        .and_then(|a| a.inventory.get(*slot))
                        .and_then(Option::as_ref)
                        .and_then(|i| session.weapons.pack.items.get(i))
                        .and_then(|i| session.weapons.pack.images.get(&i.image));
                    if live.is_none_or(|i| i.id != *image || !super::super::tools::native_hammer(i))
                    {
                        return Err(Rejection::Changed);
                    }
                    if !session.hammer_vehicle_allowed(bot, v.id.0) {
                        return Err(Rejection::Forbidden);
                    }
                    out.equip = Some(*slot);
                    let state = peer.player.state();
                    let direction = Vec3::new(
                        state.yaw.sin() * state.pitch.cos(),
                        state.pitch.sin(),
                        -state.yaw.cos() * state.pitch.cos(),
                    );
                    out.trigger = approach.pushing
                        && session.native_hammer_target(bot, direction).ok().flatten()
                            == Some(bri_weapons::TargetId::Vehicle(v.id.0));
                }
                out.point.y = feet.y;
            }
            Method::Hold {
                slot,
                image,
                descriptor,
            } => {
                if v.seats.iter().any(|s| s.occupant.is_some()) {
                    return Err(Rejection::Occupied);
                }
                let live = session
                    .weapons
                    .actor(ActorId(bot))
                    .and_then(|a| a.inventory.get(*slot))
                    .and_then(Option::as_ref)
                    .and_then(|i| session.weapons.pack.items.get(i))
                    .and_then(|i| session.weapons.pack.images.get(&i.image));
                if live.is_none_or(|i| {
                    i.id != *image || i.bot.and_then(|b| b.manipulation) != Some(*descriptor)
                }) {
                    return Err(Rejection::Changed);
                }
                let BotManipulation::Hold { near, reach, .. } = *descriptor;
                let extent =
                    (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)).length() * v.scale * 0.5;
                let desired_centre = destination + centre - at;
                // A native hold retains its acquired distance. This geometric
                // approach is corrected from that observation, not an invented
                // throw trajectory or a duplicate hold-force integrator.
                let observed_hold = session.held_by(bot);
                let held = observed_hold == Some(object);
                let distance = if held {
                    let (target, distance, grip, _) = session
                        .bot_hold_geometry(bot)
                        .ok_or(Rejection::Unsupported)?;
                    if target != object {
                        return Err(Rejection::Changed);
                    }
                    out.aim = destination + grip - at;
                    distance
                } else {
                    (desired_centre.y - peer.player.eye().y).abs().max(near) + extent + 1.0
                };
                if distance > reach {
                    return Err(Rejection::Unsupported);
                }
                let desired_grip = if held { out.aim } else { desired_centre };
                // A nearly delivered body's residual has no stable direction.
                // Correct the holder's range along its actual radial sight to
                // the desired grip instead of orbiting a jittering body axis.
                let radial = flat(desired_grip - peer.player.eye())
                    .normalize_or(flat(peer.player.state().forward()).normalize_or(Vec3::NEG_Z));
                let vertical = desired_grip.y - peer.player.eye().y;
                if held && vertical.abs() >= distance {
                    return Err(Rejection::Unsupported);
                }
                out.equip = Some(*slot);
                // The native ray may acquire an intervening object while aim
                // settles. Release that observed wrong grip ordinarily before
                // trying again; it is neither goal progress nor a throw.
                // Retain the exact grip through admission and delayed effects.
                out.trigger = observed_hold.is_none_or(|target| target == object);
                out.held = held.then_some(object);
                let eye_height = peer.player.eye().y - feet.y;
                if held {
                    let across = (distance * distance - vertical * vertical).max(0.0).sqrt();
                    out.point = desired_grip - radial * across - Vec3::Y * eye_height;
                    out.point.y = feet.y;
                    out.aim = desired_grip;
                } else {
                    let (point, ready) = hold_acquisition_approach(
                        feet,
                        centre,
                        toward,
                        distance,
                        peer.player.tuning().width,
                    )
                    .ok_or(Rejection::Unsupported)?;
                    out.point = point;
                    // Acquire at the planned standoff, rather than firing on
                    // the way in and inheriting an unrelated long grip range.
                    // A correct existing hold stays down; a wrong one releases.
                    out.trigger &= ready;
                }
            }
            Method::Drive { seat } => {
                if !session.can_ride(bot, v.owner.0)
                    || world
                        .seat_occupant(v.id, usize::from(*seat))
                        .is_some_and(|o| o.owner.0 != bot)
                {
                    return Err(Rejection::Occupied);
                }
                if session.mounted(bot) == Some((v.id.0, *seat)) {
                    out.drive = Some((v.id.0, *seat));
                    out.point = destination;
                    out.aim = destination;
                } else {
                    // Reuse the native interaction adapter's oriented chassis
                    // clearance. Root alone submits the ordinary reach/board.
                    out.point = session
                        .bot_seat_approach(bot, &v, *seat)
                        .ok_or(Rejection::Unsupported)?;
                    out.board = Some((v.id.0, *seat));
                }
            }
        }
        if !out.point.is_finite() || !out.aim.is_finite() {
            return Err(Rejection::Unsupported);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_point(feet: Vec3, centre: Vec3, toward: Vec3, radius: f32, width: f32) -> Option<Vec3> {
        push_approach(feet, centre, toward, radius, width).map(|a| a.point)
    }

    #[test]
    fn hold_acquisition_reaches_delivery_rear_without_the_old_through_body_flip() {
        for shift in [
            Vec3::ZERO,
            Vec3::new(24.0, 0.0, 0.0),
            Vec3::new(-7.3, 0.0, 11.7),
        ] {
            let centre = shift + Vec3::new(0.25, 0.8, 56.25);
            let destination = shift + Vec3::new(0.25, 0.8, 45.96);
            let toward = destination - centre;
            for z in [42.06, 45.64, 47.42, 51.6] {
                let feet = shift + Vec3::new(0.5, 0.005, z);
                let (point, ready) =
                    hold_acquisition_approach(feet, centre, toward, 4.54, 1.0).unwrap();
                assert!(
                    !ready,
                    "the acquisition must first go around to the delivery side"
                );
                let segment = flat(point - feet);
                let t = flat(centre - feet).dot(segment) / segment.length_squared();
                let closest = feet + segment * t.clamp(0.0, 1.0);
                assert!(
                    flat(closest - centre).length() > 1.6,
                    "approach crosses the expanded actual hull"
                );
                assert_eq!(point.y, feet.y);
                if feet.z > destination.z {
                    let old_radial = flat(destination - (feet + Vec3::Y * 1.6)).normalize();
                    let old_point = centre - old_radial * 4.54;
                    let chord = flat(old_point - feet);
                    let t = flat(centre - feet).dot(chord) / chord.length_squared();
                    assert!(
                        flat(feet + chord * t.clamp(0.0, 1.0) - centre).length() < 1.6,
                        "counterfactual must reproduce the old through-body shortcut"
                    );
                }
                // Test rotation in the object's local frame. Rotating absolute
                // shifted f32 positions rounds before their later subtraction;
                // that numerical cancellation is not an approach invariant.
                let local_feet = feet - centre;
                let local =
                    hold_acquisition_approach(local_feet, Vec3::ZERO, toward, 4.54, 1.0).unwrap();
                let turn = glam::Quat::from_rotation_y(0.73);
                let rotated = hold_acquisition_approach(
                    turn * local_feet,
                    Vec3::ZERO,
                    turn * toward,
                    4.54,
                    1.0,
                )
                .unwrap();
                assert!(rotated.0.abs_diff_eq(turn * local.0, 1e-5));
                assert_eq!(local.1, ready);
                assert_eq!(rotated.1, ready);
            }
            let rear = centre - flat(toward).normalize() * 4.54;
            let (point, ready) = hold_acquisition_approach(
                Vec3::new(rear.x, 0.005, rear.z),
                centre,
                toward,
                4.54,
                1.0,
            )
            .unwrap();
            assert!(ready);
            assert!(flat(point - rear).length() < 1e-5);
        }
    }

    #[test]
    fn region_origin_inclusion_and_effort_are_finite() {
        let b = (Vec3::new(-1.0, 2.0, -1.0), Vec3::new(1.0, 3.0, 1.0));
        assert!(inside(Vec3::new(0.0, 2.0, 0.0), b));
        assert!(!inside(Vec3::new(0.0, 1.9, 0.0), b));
        assert!(!inside(Vec3::NAN, b));
        assert!(effort(10.0, 8.0, 1.0) < effort(10.0, 0.2, 0.25));
        assert_eq!(effort(f32::MAX, 0.0, 0.0), 100000);
    }

    #[test]
    fn exit_geometry_is_horizontal_finite_and_clears_inclusive_region_edges() {
        let bounds = (Vec3::new(-2.0, 1.0, -3.0), Vec3::new(2.0, 5.0, 3.0));
        for point in [Vec3::new(0.0, 2.0, 0.0), Vec3::new(2.0, 4.0, 1.0)] {
            assert!(inside(point, bounds));
            let exit = exit_point(point, bounds, 0.75).unwrap();
            assert!(!inside(exit, bounds));
            assert_eq!(exit.y, point.y);
        }
        assert_eq!(exit_point(Vec3::NAN, bounds, 0.75), None);
        assert_eq!(
            exit_point(Vec3::new(0.0, 2.0, 0.0), bounds, f32::INFINITY),
            None
        );
    }

    #[test]
    fn front_and_side_approaches_clear_the_expanded_body_before_contact() {
        let radius = 1.2;
        for feet in [
            Vec3::new(0.0, 0.0, 4.0),
            Vec3::new(-2.0, 0.0, 1.0),
            Vec3::new(2.0, 0.0, 1.0),
        ] {
            let point = push_point(feet, Vec3::ZERO, Vec3::Z, radius, 1.0).unwrap();
            let segment = point - feet;
            let t = (-feet.dot(segment) / segment.length_squared()).clamp(0.0, 1.0);
            assert!(
                (feet + segment * t).length() >= radius - 1e-5,
                "approach crosses expanded hull"
            );
        }
        let left = push_point(Vec3::new(-2.0, 0.0, 1.0), Vec3::ZERO, Vec3::Z, radius, 1.0).unwrap();
        let right = push_point(Vec3::new(2.0, 0.0, 1.0), Vec3::ZERO, Vec3::Z, radius, 1.0).unwrap();
        assert!(left.abs_diff_eq(Vec3::new(-right.x, right.y, right.z), 1e-5));
        let feet = Vec3::new(1.0, 0.0, 4.0);
        let point = push_point(feet, Vec3::ZERO, Vec3::Z, radius, 1.0).unwrap();
        let rotate = glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let rotated = push_point(rotate * feet, Vec3::ZERO, rotate * Vec3::Z, radius, 1.0).unwrap();
        assert!(rotated.abs_diff_eq(rotate * point, 1e-5));
    }

    #[test]
    fn inward_and_invalid_approaches_never_request_a_through_body_shortcut() {
        let out = push_point(Vec3::new(0.0, 0.0, 0.5), Vec3::ZERO, Vec3::Z, 1.2, 1.0).unwrap();
        assert!(out.z > 1.2 && out.x == 0.0);
        assert_eq!(push_point(Vec3::NAN, Vec3::ZERO, Vec3::Z, 1.2, 1.0), None);
        assert_eq!(push_point(Vec3::Z, Vec3::ZERO, Vec3::ZERO, 1.2, 1.0), None);
        assert_eq!(
            push_point(Vec3::Z, Vec3::ZERO, Vec3::Z, f32::INFINITY, 1.0),
            None
        );
        assert_eq!(push_point(Vec3::Z, Vec3::ZERO, Vec3::Z, 1.2, 0.0), None);
    }
}
