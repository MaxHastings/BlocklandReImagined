//! What a wheeled chassis can do, measured: how far it takes to stop from
//! a speed, and how fast it holds a turn of a radius. Like a body's reach
//! ([`super::Reach`]), it is measured once per definition and scale of a
//! loaded vehicle pack by running the real vehicle code on a bare test
//! floor, under the controls a bot drives with (its throttle,
//! [`crate::route::DRIVE_THROTTLE`], the brake, mouse steering that holds
//! its lock), and shared. Mass, engine, brakes, tyres, steering and scale all
//! change the result with no bot support of their own. It sees no map:
//! what lies ahead stays the driver's hull checks.
use bri_physics::FIXED_DT;
use bri_vehicles::{
    Controls, Definition, Occupant, OccupantId, OwnerId, Pack, Spawn, Transform, VehicleId,
    VehiclesWorld,
};
use glam::Vec3;
use rapier3d::prelude::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle};
use std::sync::{Arc, Mutex};

/// Speeds a turn is held at, apart.
const TURN_SPACING: f32 = 2.0;
/// It has reached its top speed once one more second adds less than this
/// share of [`TURN_SPACING`].
const STEADY: f32 = 0.25;
/// How long it holds each speed before its turn is read (while the turn
/// settles), and how long the turn is read over.
const HOLD_SECONDS: f32 = 0.5;
const READ_SECONDS: f32 = 0.5;
/// Below this speed it has stopped.
const STOPPED: f32 = 0.1;
/// The longest any one run drives for.
const MAX_SECONDS: f32 = 30.0;
/// The longest it is given to settle on its wheels once dropped.
const SETTLE_SECONDS: f32 = 2.0;
/// The shares of its full lock it is measured turning at: the turns
/// between come from these, and from driving straight.
const LOCKS: [f32; 3] = [1.0, 0.5, 0.25];
/// Half the side of the test floor.
const FLOOR: f32 = 4000.0;
/// The measuring run's own vehicle, driver and seat.
const VEHICLE: VehicleId = VehicleId(1);
const DRIVER: OccupantId = OccupantId(1);
const OWNER: OwnerId = OwnerId(1);

/// A wheeled chassis's measured stopping and turning.
#[derive(Clone, Debug, PartialEq)]
pub struct Handling {
    /// Its top speed on the flat at a driver's throttle.
    pub top: f32,
    /// Its tightest turn's radius (full lock, slowest).
    pub tightest: f32,
    /// (speed, distance from there to a stop under the brake), by speed.
    stops: Vec<(f32, f32)>,
    /// (curvature, the fastest it holds a turn that tight), curvature
    /// falling to 0 at its top speed.
    turns: Vec<(f32, f32)>,
}

/// One measured definition and scale of one loaded pack (its
/// fingerprint): a reloaded pack is measured again.
type Measured = (String, String, u32, Option<Arc<Handling>>);
static MEASURED: Mutex<Vec<Measured>> = Mutex::new(Vec::new());

impl Handling {
    /// The handling of `definition` (one of `vehicles`' pack) at `scale`,
    /// measured the first time it is asked for; `None` for one that does
    /// not drive (it cannot be spawned and driven on a floor, or does not
    /// move).
    pub fn of(vehicles: &VehiclesWorld, definition: &Definition, scale: f32) -> Option<Arc<Self>> {
        let pack = vehicles.catalog_fingerprint();
        let mut measured = MEASURED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((.., handling)) = measured
            .iter()
            .find(|(p, id, s, _)| *s == scale.to_bits() && id == &definition.id && p == pack)
        {
            return handling.clone();
        }
        let handling = Self::measure(definition, scale).map(Arc::new);
        measured.push((
            pack.to_owned(),
            definition.id.clone(),
            scale.to_bits(),
            handling.clone(),
        ));
        handling
    }

    /// Measures it, sharing nothing.
    pub fn measure(definition: &Definition, scale: f32) -> Option<Self> {
        let (top, stops) = straight(definition, scale)?;
        let mut samples: Vec<(f32, f32)> = LOCKS
            .iter()
            .filter_map(|&lock| turning(definition, scale, lock))
            .flatten()
            .collect();
        let tightest = samples.iter().map(|s| s.0).fold(0.0, f32::max);
        if top < TURN_SPACING || tightest <= 0.0 {
            return None;
        }
        samples.push((0.0, top));
        // The fastest it holds each turn, or any tighter one.
        samples.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut turns: Vec<(f32, f32)> = Vec::with_capacity(samples.len());
        for (curvature, speed) in samples {
            let speed = turns.last().map_or(speed, |l| speed.max(l.1));
            match turns.last_mut() {
                Some(l) if l.0 == curvature => l.1 = speed,
                _ => turns.push((curvature, speed)),
            }
        }
        Some(Self {
            top,
            tightest: 1.0 / tightest,
            stops,
            turns,
        })
    }

    /// How far it rolls under the brake from `speed`. Faster than it was
    /// measured stopping from, the last stop grows with the square.
    pub fn stopping(&self, speed: f32) -> f32 {
        let speed = speed.abs();
        let Some(&(fastest, longest)) = self.stops.last() else {
            return 0.0;
        };
        if speed >= fastest {
            return longest * (speed / fastest).powi(2);
        }
        let mut below = (0.0, 0.0);
        for &(s, d) in &self.stops {
            if s >= speed {
                let t = (speed - below.0) / (s - below.0).max(f32::EPSILON);
                return below.1 + (d - below.1) * t;
            }
            below = (s, d);
        }
        longest
    }

    /// The fastest it stops from within `distance`: [`Self::stopping`]
    /// the other way round.
    pub fn stopping_speed(&self, distance: f32) -> f32 {
        let distance = distance.max(0.0);
        let Some(&(fastest, longest)) = self.stops.last() else {
            return 0.0;
        };
        if distance >= longest {
            return fastest * (distance / longest.max(f32::EPSILON)).sqrt();
        }
        let mut below = (0.0, 0.0);
        for &(s, d) in &self.stops {
            if d >= distance {
                let t = (distance - below.1) / (d - below.1).max(f32::EPSILON);
                return below.0 + (s - below.0) * t;
            }
            below = (s, d);
        }
        fastest
    }

    /// The fastest it holds a turn of `radius`, interpolated between the
    /// turns it was measured at; a turn tighter than it makes, at its
    /// slowest.
    pub fn arc_speed(&self, radius: f32) -> f32 {
        let curvature = 1.0 / radius.max(f32::EPSILON);
        let mut tighter = self.turns[0];
        if curvature >= tighter.0 {
            return tighter.1;
        }
        for &(c, s) in &self.turns[1..] {
            if c <= curvature {
                let t = (tighter.0 - curvature) / (tighter.0 - c).max(f32::EPSILON);
                return tighter.1 + (s - tighter.1) * t;
            }
            tighter = (c, s);
        }
        self.top
    }

    /// The fastest it still turns as tightly as it can, give or take
    /// `slack` on the radius: the speed for backing out of or pulling out
    /// of its circle.
    pub fn manoeuvre_speed(&self, slack: f32) -> f32 {
        self.arc_speed(self.tightest + slack)
    }
}

/// The test floor with the vehicle settled on it and a driver in its seat.
fn stand(definition: &Definition, scale: f32) -> Option<(VehiclesWorld, PhysicsWorld)> {
    let mut vehicles = VehiclesWorld::new(Pack {
        schema_version: bri_vehicles::schema::SCHEMA_VERSION,
        definitions: vec![definition.clone()],
        assets: vec![],
        evidence: vec![],
        unresolved: vec![],
        animation_aliases: Default::default(),
    })
    .ok()?;
    let mut world = bri_physics::new_world();
    world.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0.0, -0.5, 0.0)),
        ColliderBuilder::cuboid(FLOOR, 0.5, FLOOR),
    );
    let height = (definition.bounds_max[1] - definition.bounds_min[1]) * scale;
    vehicles
        .spawn(
            &mut world,
            Spawn {
                scale,
                id: VEHICLE,
                owner: OWNER,
                definition: definition.id.clone(),
                transform: Transform {
                    position: [0.0, height, 0.0],
                    ..Default::default()
                },
                spawn_id: None,
                respawn_ticks: None,
            },
        )
        .ok()?;
    world.detect_collisions(&(), &());
    let seat = definition.seats.iter().position(|s| s.controls)?;
    let at = vehicles.snapshot(&world).vehicles[0].seats[seat]
        .transform
        .position;
    vehicles
        .mount(
            &world,
            VEHICLE,
            seat,
            Occupant {
                id: DRIVER,
                owner: OWNER,
                body: [1.0, 2.0],
            },
            at,
        )
        .ok()?;
    let mut run = Run::new(vehicles, world)?;
    for _ in 0..ticks(SETTLE_SECONDS) {
        let state = run.step(Controls::default())?;
        if state.speed < STOPPED && state.velocity.y.abs() < STOPPED {
            break;
        }
    }
    Some((run.vehicles, run.world))
}

fn ticks(seconds: f32) -> usize {
    (seconds / FIXED_DT).ceil() as usize
}

/// The chassis as one tick left it.
struct State {
    at: Vec3,
    velocity: Vec3,
    /// Flat speed.
    speed: f32,
    upright: bool,
}

struct Run {
    vehicles: VehiclesWorld,
    world: PhysicsWorld,
    body: RigidBodyHandle,
}
impl Run {
    fn new(vehicles: VehiclesWorld, world: PhysicsWorld) -> Option<Self> {
        let body = vehicles.body_of(VEHICLE)?;
        Some(Self {
            vehicles,
            world,
            body,
        })
    }
    /// One tick under `controls`, with its steering prefs off as a bot's
    /// are: the mouse steers, and the lock holds.
    fn step(&mut self, controls: Controls) -> Option<State> {
        let controls = Controls {
            strafe_steering_off: true,
            auto_return_off: true,
            ..controls
        };
        self.vehicles.set_controls(OWNER, DRIVER, controls).ok()?;
        self.vehicles.pre_step(&mut self.world, &[]).ok()?;
        self.world.step();
        self.vehicles.post_step(&mut self.world).ok()?;
        let body = self.world.bodies.get(self.body)?;
        let velocity = body.linvel();
        let rotation = body.rotation();
        Some(State {
            at: body.translation(),
            velocity,
            speed: Vec3::new(velocity.x, 0.0, velocity.z).length(),
            upright: (rotation * Vec3::Y).y >= 0.5,
        })
    }
    /// A driver's throttle straight ahead until its speed stops rising:
    /// its top speed, unless it tipped over.
    fn top_speed(&mut self) -> Option<f32> {
        let mut last_second = 0.0;
        for tick in 1..=ticks(MAX_SECONDS) {
            let state = self.step(Controls {
                throttle: crate::route::DRIVE_THROTTLE,
                ..Default::default()
            })?;
            if !state.upright {
                return None;
            }
            if tick % ticks(1.0) == 0 {
                if state.speed - last_second < TURN_SPACING * STEADY {
                    return Some(state.speed);
                }
                last_second = state.speed;
            }
        }
        None
    }
}

/// Its top speed driving straight, then braking from it: each speed it
/// fell through and how far it rolled from there to a stop.
fn straight(definition: &Definition, scale: f32) -> Option<(f32, Vec<(f32, f32)>)> {
    let (vehicles, world) = stand(definition, scale)?;
    let mut run = Run::new(vehicles, world)?;
    let top = run.top_speed()?;
    let mut trail: Vec<(f32, Vec3)> = vec![];
    let mut stopped = None;
    for _ in 0..ticks(MAX_SECONDS) {
        let state = run.step(Controls {
            brake: true,
            ..Default::default()
        })?;
        trail.push((state.speed, state.at));
        if state.speed < STOPPED {
            stopped = Some(state.at);
            break;
        }
    }
    let end = stopped?;
    let mut stops: Vec<(f32, f32)> = trail
        .iter()
        .map(|&(speed, at)| (speed, flat(end - at).length()))
        .collect();
    stops.reverse();
    // By speed, never shorter from faster.
    let mut longest: f32 = 0.0;
    stops.retain_mut(|(speed, d)| {
        longest = longest.max(*d);
        *d = longest;
        *speed >= STOPPED
    });
    stops.dedup_by(|a, b| a.0 <= b.0);
    Some((top, stops))
}

/// At `lock` (a share of its full lock), holding each speed
/// [`TURN_SPACING`] apart in turn: the curvature of its path once the turn
/// has settled, as (curvature, speed held). It stops at the first speed it
/// cannot reach or hold on that lock, or where it tips over.
fn turning(definition: &Definition, scale: f32, lock: f32) -> Option<Vec<(f32, f32)>> {
    let (vehicles, world) = stand(definition, scale)?;
    let mut run = Run::new(vehicles, world)?;
    let mut look = definition.max_steering * lock;
    let mut samples = vec![];
    let mut speed = 0.0;
    let mut budget = ticks(MAX_SECONDS);
    let mut target = TURN_SPACING;
    loop {
        // Up to it, unless it stops gaining.
        let mut last_second = speed;
        let mut tick = 0;
        while speed < target {
            let state = run.step(Controls {
                throttle: crate::route::DRIVE_THROTTLE,
                look_delta: [look, 0.0],
                ..Default::default()
            })?;
            look = 0.0;
            speed = state.speed;
            tick += 1;
            let Some(left) = budget.checked_sub(1) else {
                return Some(samples);
            };
            budget = left;
            if !state.upright {
                return Some(samples);
            }
            if tick % ticks(1.0) == 0 {
                if speed - last_second < TURN_SPACING * STEADY {
                    return Some(samples);
                }
                last_second = speed;
            }
        }
        // Held there: the throttle while below it, the turn read once it
        // has settled.
        let (mut turned, mut travelled, mut held) = (0.0, 0.0, 0.0);
        let mut last: Option<(Vec3, f32)> = None;
        let reading = ticks(READ_SECONDS);
        for tick in 0..ticks(HOLD_SECONDS) + reading {
            let state = run.step(Controls {
                throttle: if speed < target {
                    crate::route::DRIVE_THROTTLE
                } else {
                    0.0
                },
                ..Default::default()
            })?;
            speed = state.speed;
            let Some(left) = budget.checked_sub(1) else {
                return Some(samples);
            };
            budget = left;
            if !state.upright {
                return Some(samples);
            }
            if tick < ticks(HOLD_SECONDS) || speed < STOPPED {
                continue;
            }
            let heading = state.velocity.x.atan2(state.velocity.z);
            if let Some((at, was)) = last {
                turned += wrap(heading - was);
                travelled += flat(state.at - at).length();
            }
            last = Some((state.at, heading));
            held += speed / reading as f32;
        }
        if travelled > 0.0 {
            samples.push((turned.abs() / travelled, held));
        }
        target += TURN_SPACING;
    }
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

fn wrap(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (a + PI).rem_euclid(TAU) - PI
}

/// The synthetic catalog's car (`bri_vehicles::testing::CAR`), changed by
/// `edit`.
#[cfg(test)]
pub(crate) fn test_car(edit: impl Fn(&mut Definition)) -> Definition {
    let mut d = bri_vehicles::testing::definitions()
        .into_iter()
        .find(|d| d.id == bri_vehicles::testing::CAR)
        .expect("the synthetic catalog has a car");
    edit(&mut d);
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_car as car;

    #[test]
    fn the_test_car_stops_and_turns_as_it_was_built_to() {
        let d = car(|_| {});
        let h = Handling::measure(&d, 1.0).expect("it drives");
        assert!(
            h.top > d.max_speed * 0.5 && h.top <= d.max_speed * 1.05,
            "{}",
            h.top
        );
        // Never shorter from faster, never tighter faster.
        assert!(h.stopping(h.top) > h.stopping(h.top * 0.5));
        assert!(h.stopping(h.top * 0.5) > 0.0);
        let d = h.stopping(h.top * 0.5);
        assert!((h.stopping_speed(d) - h.top * 0.5).abs() < 0.1);
        assert!(h.arc_speed(h.tightest * 4.0) >= h.manoeuvre_speed(0.0));
        assert!(h.arc_speed(1e6) <= h.top * 1.0001);
    }

    #[test]
    fn a_measured_definition_is_shared_and_a_reloaded_pack_measured_again() {
        let world = |edit: fn(&mut Definition)| {
            VehiclesWorld::new(bri_vehicles::testing::pack_with(|d| {
                if d.id == bri_vehicles::testing::CAR {
                    edit(d)
                }
            }))
            .unwrap()
        };
        let (plain, braver) = (world(|_| {}), world(|d| d.brake_force *= 2.0));
        let of = |w: &VehiclesWorld| {
            Handling::of(w, w.definition(bri_vehicles::testing::CAR).unwrap(), 1.0).unwrap()
        };
        let a = of(&plain);
        assert!(Arc::ptr_eq(&a, &of(&plain)));
        assert!(!Arc::ptr_eq(&a, &of(&braver)));
    }

    #[test]
    fn stronger_brakes_stop_shorter() {
        let (weak, strong) = (
            Handling::measure(&car(|_| {}), 1.0).unwrap(),
            Handling::measure(&car(|d| d.brake_force *= 3.0), 1.0).unwrap(),
        );
        let speed = weak.top.min(strong.top) * 0.8;
        assert!(strong.stopping(speed) < weak.stopping(speed));
    }

    #[test]
    fn more_lock_turns_tighter() {
        let (less, more) = (
            Handling::measure(&car(|d| d.max_steering *= 0.5), 1.0).unwrap(),
            Handling::measure(&car(|_| {}), 1.0).unwrap(),
        );
        assert!(
            more.tightest < less.tightest,
            "{} {}",
            more.tightest,
            less.tightest
        );
    }
}
