//! Measured reach: what a body's own motor really does, flown once per
//! mechanical setup on a bare test floor and kept (`docs/architecture/bots.md`,
//! Routes).
//!
//! The route planner asks what a body can climb and how long a jet leg
//! takes. Rather than a physics formula with correction factors, the answer
//! comes from running the real player motor ([`Player::step`]) under the same
//! leg controller a bot flies with ([`crate::route::jet`]), in an isolated
//! world holding nothing but a floor and the ledge or landing being tried.
//! The result depends only on the body's tuning (gravity, jets, jump, size),
//! so a modded body, a low-gravity game or a scaled player each get their
//! own measurement with no bot support of their own.
//!
//! What it does not see is the real map: a low ceiling or a crowd over the
//! takeoff. Geometry stays the planner's ([`crate::nav`]) and what really
//! happens on a leg stays the leg's ([`crate::route::JetLeg::failed`]).
use crate::route::{JET_CLEARANCE, JetLeg};
use bri_motor::player::{MoveInput, Player, PlayerTuning};
use bri_physics::FIXED_DT;
use glam::Vec3;
use rapier3d::prelude::*;
use std::sync::{Arc, LazyLock, Mutex};

mod handling;
pub use handling::Handling;
#[cfg(test)]
pub(crate) use handling::test_car;

/// Ticks of the motor a second.
const TICKS: f32 = 1.0 / FIXED_DT;
/// Spacing of the heights and distances a jet leg is flown over; between
/// them a flight's time is interpolated.
const SPACING: f32 = 2.0;
/// Seconds per unit that count as the same: once one more spacing of climb
/// or of crossing adds the same time as the last did, farther legs are
/// taken to go on at that rate. A crossing has reached its top speed; a
/// climb may still be gaining, so past the grid it is if anything quicker
/// than the planner thinks.
const STEADY: f32 = 0.005;
/// The longest a single measured run may go on: a body still not up or
/// across by then cannot make the leg (its jets barely lift it).
const MAX_SECONDS: f32 = 30.0;
/// The most spacings a jet envelope measures up or across before it takes
/// the rate it has reached as the rate from there on.
const MAX_SAMPLES: usize = 16;
/// How long a walk leg goes on jumping at a ledge it lands short of: until
/// its stall judge has seen two windows go nowhere and plans again.
const RETRY_SECONDS: f32 = 2.0 * crate::route::Progress::WINDOW;
/// How close a ledge's measured height is pinned down.
const LEDGE_TOLERANCE: f32 = 0.01;
/// How far ahead of the ledge's edge a body standing on it puts its feet:
/// a nav cell, the nearest floor the grid sends it to.
const ON_LEDGE: f32 = crate::nav::CELL;

/// What a body with one tuning can do, as measured.
#[derive(Debug, PartialEq)]
pub struct Reach {
    /// Highest ledge a jump from a walk lands it on.
    pub ledge: f32,
    /// Highest crawlspace floor (a window up a wall) a jump from a walk,
    /// crouching once off the ground, gets it into.
    pub crawl_ledge: f32,
    /// Highest crawlspace floor it walks up into, crouched, without a
    /// jump (the step it climbs with its head under a roof).
    pub crawl_step: f32,
    /// What its jets do with an endless supply of energy; `None` when they
    /// do not lift it (or it has none).
    pub jets: Option<JetReach>,
    /// Its jumps across open air, from one support onto another.
    pub leaps: Arc<Leaps>,
}

/// The jumps a body makes across open air, measured: for each rise (up a
/// ledge, or down across a gap), how far off it lands standing on a
/// landing, and how long that takes. Measured from standing still on a
/// takeoff no bigger than a landing (a peg, a brick ledge gives no
/// run-up), flown by the bot's own leap controller ([`crate::route::leap`]).
#[derive(Debug, PartialEq)]
pub struct Leaps {
    /// Rise of each row, lowest first, [`LEAP_SPACING`] apart.
    rises: Vec<f32>,
    /// Per rise, its landings: (centre distance, seconds), nearest first,
    /// [`LEAP_SPACING`] apart; empty when it lands none.
    rows: Vec<Vec<(f32, f32)>>,
}
/// Tunings whose reach is kept measured at once: a game holds a handful,
/// and one asked for again after more than this many others is measured
/// again.
const MEASURED_KEPT: usize = 64;
/// Spacing of the rises and distances a leap is measured over: a nav cell,
/// the grid the planner asks about.
const LEAP_SPACING: f32 = crate::nav::CELL;
impl Leaps {
    /// A body's that leaps nowhere (a chassis).
    pub fn none() -> Arc<Self> {
        static NONE: LazyLock<Arc<Leaps>> = LazyLock::new(|| {
            Arc::new(Leaps {
                rises: Vec::new(),
                rows: Vec::new(),
            })
        });
        NONE.clone()
    }
    fn measure(tuning: &PlayerTuning) -> Self {
        // From as far down as its jump goes up to the highest ledge it
        // could reach with a step on top.
        let apex = crate::route::Motion::of(tuning).apex();
        let lowest = -(apex / LEAP_SPACING).ceil() as i32;
        let highest = ((apex + tuning.step_height) / LEAP_SPACING).ceil() as i32;
        let rises: Vec<f32> = (lowest..=highest)
            .map(|k| k as f32 * LEAP_SPACING)
            .collect();
        let row = |rise: f32| {
            // Out from where the landing touches the takeoff, as far as a
            // walk at its speed carries it over the hop's time in the air.
            let first = landing_width(tuning);
            let flight = crate::route::Motion::of(tuning).hop(0.0, rise);
            let farthest = first + tuning.forward * flight;
            let mut out = Vec::new();
            let mut across = first;
            while across <= farthest {
                match leap(tuning, rise, across) {
                    Some(seconds) => out.push((across, seconds)),
                    // Past the farthest it lands: no farther either.
                    None if !out.is_empty() => break,
                    None => {}
                }
                across += LEAP_SPACING;
            }
            out
        };
        let rows = rises.iter().map(|r| row(*r)).collect();
        Self { rises, rows }
    }
    /// Seconds a leap takes onto a landing `rise` above (below) the
    /// takeoff whose spot is `across` away; `None` when the body does not
    /// land there. A rise between two measured is judged as the higher,
    /// and one below the lowest as the lowest (falling farther, it only
    /// reaches farther).
    pub fn seconds(&self, rise: f32, across: f32) -> Option<f32> {
        let i = self.rises.iter().position(|r| *r >= rise - f32::EPSILON)?;
        let row = &self.rows[i];
        let (first, last) = (row.first()?, row.last()?);
        if across < first.0 - LEAP_SPACING * 0.5 || across > last.0 + LEAP_SPACING * 0.5 {
            return None;
        }
        row.iter()
            .min_by(|a, b| (a.0 - across).abs().total_cmp(&(b.0 - across).abs()))
            .map(|(_, seconds)| *seconds)
    }
    /// The farthest any leap lands, centre to centre (zero: none does).
    pub fn farthest(&self) -> f32 {
        self.rows
            .iter()
            .filter_map(|row| row.last())
            .map(|(across, _)| *across)
            .fold(0.0, f32::max)
    }
}

/// One jet leg as flown: from takeoff to standing on the landing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    /// Seconds from takeoff to standing on the landing.
    pub seconds: f32,
    /// Of those, seconds with the jets on: what its energy must hold.
    pub jetting: f32,
}

/// Jet legs flown from a standing takeoff over a grid of climbs and
/// crossings ([`SPACING`] apart, from the lowest climb worth jetting and
/// the nearest landing it can lift off beside), out to where one more
/// spacing adds a steady time ([`STEADY`]).
#[derive(Debug, PartialEq)]
pub struct JetReach {
    first_up: f32,
    first_across: f32,
    /// The highest climb it made, when a higher one failed: above it a leg
    /// is not one it can fly. `None`: every climb measured made it, and
    /// higher ones only take longer.
    highest: Option<f32>,
    /// `flights[i][j]`: a climb of `first_up + i * SPACING` crossing
    /// `first_across + j * SPACING`.
    flights: Vec<Vec<Flight>>,
}

impl Reach {
    /// The measured reach of a body with `tuning`: measured the first time
    /// any body with that tuning asks, then shared while it is among the
    /// [`MEASURED_KEPT`] asked for last.
    pub fn of(tuning: &PlayerTuning) -> Arc<Self> {
        // Every field of a tuning is mechanical: the same tuning is the
        // same reach. Most recently asked last.
        static MEASURED: Mutex<Vec<(PlayerTuning, Arc<Reach>)>> = Mutex::new(Vec::new());
        let found = |measured: &mut Vec<(PlayerTuning, Arc<Reach>)>| {
            let at = measured.iter().position(|(t, _)| t == tuning)?;
            let entry = measured.remove(at);
            let reach = entry.1.clone();
            measured.push(entry);
            Some(reach)
        };
        if let Some(reach) = found(&mut MEASURED.lock().unwrap()) {
            return reach;
        }
        // Measured outside the lock: another tuning's measurement need not
        // wait on this one.
        let reach = Arc::new(Self::measure(tuning));
        let mut measured = MEASURED.lock().unwrap();
        if let Some(reach) = found(&mut measured) {
            return reach;
        }
        if measured.len() >= MEASURED_KEPT {
            measured.remove(0);
        }
        measured.push((tuning.clone(), reach.clone()));
        reach
    }
    /// Measures `tuning` afresh.
    pub fn measure(tuning: &PlayerTuning) -> Self {
        Self {
            ledge: ledge(tuning, false),
            crawl_ledge: ledge(tuning, true),
            crawl_step: crawl_step(tuning),
            jets: JetReach::measure(tuning),
            leaps: Arc::new(Leaps::measure(tuning)),
        }
    }
}

impl JetReach {
    fn measure(tuning: &PlayerTuning) -> Option<Self> {
        if !tuning.can_jet {
            return None;
        }
        // The nearest landing it can take off beside, and the lowest one
        // worth jetting to (anything lower it steps up).
        let first_across = landing_width(tuning) * 0.5 + tuning.width * 0.5 + ON_LEDGE;
        let first_up = tuning.step_height + SPACING;
        let flight = |i: usize, j: usize| {
            fly(
                tuning,
                first_up + i as f32 * SPACING,
                first_across + j as f32 * SPACING,
            )
        };
        let mut flights = vec![vec![flight(0, 0)?]];
        let mut highest = None;
        // Across at the lowest climb until crossing farther adds a steady
        // time, then up, each height across as many as the first.
        while flights[0].len() < MAX_SAMPLES && !steady(&flights[0]) {
            let next = flight(0, flights[0].len())?;
            flights[0].push(next);
        }
        let column = |f: &[Vec<Flight>]| f.iter().map(|row| row[0]).collect::<Vec<_>>();
        while flights.len() < MAX_SAMPLES && !steady(&column(&flights)) {
            let i = flights.len();
            let row: Option<Vec<_>> = (0..flights[0].len()).map(|j| flight(i, j)).collect();
            match row {
                Some(row) => flights.push(row),
                // It cannot climb that high: no higher either.
                None => {
                    highest = Some(first_up + (i - 1) as f32 * SPACING);
                    break;
                }
            }
        }
        Some(Self {
            first_up,
            first_across,
            highest,
            flights,
        })
    }
    /// The leg climbing `up` and crossing `across`, from the measured
    /// flights around it; past the last measured height or distance at the
    /// rate it had reached.
    pub fn flight(&self, up: f32, across: f32) -> Flight {
        let (i, s) = cell(up, self.first_up, self.flights.len());
        let (j, t) = cell(across, self.first_across, self.flights[0].len());
        let at = |i: usize, j: usize| {
            let row = &self.flights[i.min(self.flights.len() - 1)];
            row[j.min(row.len() - 1)]
        };
        let mix = |a: Flight, b: Flight, w: f32| Flight {
            seconds: a.seconds + (b.seconds - a.seconds) * w,
            jetting: a.jetting + (b.jetting - a.jetting) * w,
        };
        let low = mix(at(i, j), at(i, j + 1), t);
        let high = mix(at(i + 1, j), at(i + 1, j + 1), t);
        let f = mix(low, high, s);
        // Never quicker than the quickest leg measured.
        let shortest = self.shortest();
        Flight {
            seconds: f.seconds.max(shortest.seconds),
            jetting: f.jetting.max(shortest.jetting),
        }
    }
    /// The highest climb it can make, when there is one.
    pub fn highest(&self) -> Option<f32> {
        self.highest
    }
    /// The quickest leg it flies: the lowest, nearest landing.
    pub fn shortest(&self) -> Flight {
        self.flights[0][0]
    }
}

/// Where `x` falls among `count` samples spaced [`SPACING`] from `first`:
/// the sample below and how far on to the next, which past the last runs on
/// at the last pair's rate (and before the first stays at the first).
fn cell(x: f32, first: f32, count: usize) -> (usize, f32) {
    if count < 2 {
        return (0, 0.0);
    }
    let along = ((x - first) / SPACING).max(0.0);
    let i = (along.floor() as usize).min(count - 2);
    (i, along - i as f32)
}

/// Whether the last spacing of `flights` added the same time as the one
/// before: the rate has settled.
fn steady(flights: &[Flight]) -> bool {
    let n = flights.len();
    n >= 3 && {
        let rate = |a: Flight, b: Flight| (b.seconds - a.seconds) / SPACING;
        (rate(flights[n - 2], flights[n - 1]) - rate(flights[n - 3], flights[n - 2])).abs() < STEADY
    }
}

/// The landing a jet leg is measured onto: the narrowest floor a body
/// stands on, a cell wider than itself.
fn landing_width(tuning: &PlayerTuning) -> f32 {
    tuning.width + crate::nav::CELL
}

/// A world with a floor whose top is at height zero and `boxes` (min, max).
fn world(boxes: &[(Vec3, Vec3)]) -> PhysicsWorld {
    world_over(boxes, 0.0)
}
/// A world with a floor whose top is at `floor` and `boxes` (min, max).
fn world_over(boxes: &[(Vec3, Vec3)], floor: f32) -> PhysicsWorld {
    let mut physics = bri_physics::new_world();
    let floor = (
        Vec3::new(-500.0, floor - 1.0, -500.0),
        Vec3::new(500.0, floor, 500.0),
    );
    for (min, max) in std::iter::once(&floor).chain(boxes) {
        let half = (*max - *min) * 0.5;
        let centre = (*min + *max) * 0.5;
        physics.insert_collider(
            ColliderBuilder::cuboid(half.x, half.y, half.z)
                .translation(Vector::new(centre.x, centre.y, centre.z)),
            None,
        );
    }
    bri_physics::detect_collisions(&mut physics);
    physics
}

/// The buttons held for one measured tick.
#[derive(Clone, Copy, Default)]
struct Press {
    jump: bool,
    crouch: bool,
    jet: bool,
}

/// One tick of `player` moving along flat `direction` (facing -z) with
/// `press` held.
fn step(player: &mut Player, physics: &mut PhysicsWorld, direction: Vec3, press: Press) {
    player
        .step(
            physics,
            MoveInput {
                // Yaw zero faces -z with +x on the right.
                forward: (-direction.z).clamp(-1.0, 1.0),
                right: direction.x.clamp(-1.0, 1.0),
                jump: press.jump,
                crouch: press.crouch,
                jet: press.jet,
                ..Default::default()
            },
        )
        .expect("a measured move is valid");
    physics.step();
}

/// A standing body with `tuning` at the origin of `physics`, settled on the
/// floor.
fn stand(physics: &mut PhysicsWorld, tuning: &PlayerTuning) -> Option<Player> {
    let mut player = Player::spawn(physics, 1, Vec3::new(0.0, 0.05, 0.0), tuning.clone()).ok()?;
    // Down onto the floor, within a second.
    for _ in 0..TICKS as usize {
        if player.state().grounded {
            return Some(player);
        }
        step(&mut player, physics, Vec3::ZERO, Press::default());
    }
    None
}

/// A jet leg flown from standing on the floor to a landing `up` high whose
/// centre is `across` away, as a bot flies one; `None` when it never stands
/// on it.
fn fly(tuning: &PlayerTuning, up: f32, across: f32) -> Option<Flight> {
    let half = landing_width(tuning) * 0.5;
    // The landing off along -z, the way the body faces.
    let to = Vec3::new(0.0, up, -across);
    let mut physics = world(&[(
        Vec3::new(-half, -1.0, -across - half),
        Vec3::new(half, up, -across + half),
    )]);
    let mut player = stand(&mut physics, tuning)?;
    let apex = up + JET_CLEARANCE;
    let start = Vec3::from(player.state().feet);
    let mut leg = JetLeg::start(to, start, 0);
    let mut jetting = 0;
    for tick in 1..=(MAX_SECONDS * TICKS) as u64 {
        let state = player.state();
        let feet = Vec3::from(state.feet);
        let grounded = state.grounded;
        if grounded && leg.crossing && (feet.y - up).abs() < tuning.step_height {
            return Some(Flight {
                seconds: tick as f32 / TICKS,
                jetting: jetting as f32 / TICKS,
            });
        }
        if leg.failed(feet, grounded, tuning.step_height, f32::INFINITY, tick) {
            return None;
        }
        let velocity = Vec3::from(state.velocity);
        let control = crate::route::jet(&mut leg, feet, velocity, grounded, to, apex, tuning);
        jetting += u64::from(control.jet);
        step(
            &mut player,
            &mut physics,
            control.direction,
            Press {
                jump: control.jump,
                jet: control.jet,
                ..Press::default()
            },
        );
    }
    None
}

/// A leap as a bot makes one: standing still on a takeoff the size of a
/// landing, it jumps onto a landing `landing_width` across whose centre is
/// `across` away and `rise` above (below) it, with nothing but a fall
/// below, flown by the bot's own leap controller. Seconds from the jump to
/// standing on the landing; `None` when it never does.
fn leap(tuning: &PlayerTuning, rise: f32, across: f32) -> Option<f32> {
    let half = landing_width(tuning) * 0.5;
    // Everything over a void: a body that misses falls far below.
    let void = rise.min(0.0) - tuning.stand_height * 4.0;
    let takeoff = (Vec3::new(-half, void, -half), Vec3::new(half, 0.0, half));
    let landing = (
        Vec3::new(-half, void, -across - half),
        Vec3::new(half, rise, -across + half),
    );
    let mut physics = world_over(&[takeoff, landing], void - 1.0);
    let mut player = stand(&mut physics, tuning)?;
    let target = Vec3::new(0.0, rise, -across);
    let feet = Vec3::from(player.state().feet);
    let mut leg = crate::route::LeapLeg::start(target, feet, feet, tuning, 0);
    for tick in 0..(MAX_SECONDS * TICKS) as usize {
        let state = player.state();
        let feet = Vec3::from(state.feet);
        if state.grounded && leg.flown {
            let at = leg.jumped? as usize;
            // Down: on the landing, or anywhere else (a miss).
            let on = (feet.y - rise).abs() < tuning.step_height
                && (feet.z + across).abs() <= half
                && feet.x.abs() <= half;
            return on.then(|| (tick - at) as f32 / TICKS);
        }
        if feet.y < void + tuning.step_height {
            return None;
        }
        let control = crate::route::leap(
            &mut leg,
            feet,
            Vec3::from(state.velocity),
            state.grounded,
            tuning,
            tick as u64,
        );
        // Off the takeoff without jumping: it fell.
        if !state.grounded && leg.jumped.is_none() {
            return None;
        }
        step(
            &mut player,
            &mut physics,
            control.direction,
            Press {
                jump: control.jump,
                ..Press::default()
            },
        );
    }
    None
}

/// How tall a crawlspace a jump into one is measured through: the crouched
/// body and half a nav cell, the grid's slack. A taller one is easier.
fn crawlspace(tuning: &PlayerTuning) -> f32 {
    tuning.crouch_height + crate::nav::CELL * 0.5
}

/// Whether a body with `tuning` walking at a ledge `height` high and
/// jumping as a bot does lands on top of it; with `crawl`, into a
/// [`crawlspace`] over it (a window up a wall), crouching once off the
/// ground as the walk leg does.
fn lands_on(tuning: &PlayerTuning, height: f32, crawl: bool) -> bool {
    // A run-up long enough to reach its walking speed.
    let edge = tuning.forward.max(1.0);
    let top = Vec3::new(0.0, height, -edge - ON_LEDGE);
    let far = Vec3::new(50.0, height, -edge);
    let near = Vec3::new(-50.0, -1.0, -edge - 50.0);
    let roof = height + crawlspace(tuning);
    let mut boxes = vec![(near, far)];
    if crawl {
        boxes.push((
            Vec3::new(near.x, roof, near.z),
            Vec3::new(far.x, roof + 50.0, far.z),
        ));
    }
    let mut physics = world(&boxes);
    let Some(mut player) = stand(&mut physics, tuning) else {
        return false;
    };
    // Whether it has jumped and left the floor, and when it first came
    // down short: a walk leg jumps again each time it lands short, until
    // its stall judge gives the leg up.
    let (mut flown, mut short) = (false, None);
    for tick in 0..(MAX_SECONDS * TICKS) as usize {
        let state = player.state();
        let feet = Vec3::from(state.feet);
        let toward = Vec3::new(0.0, 0.0, top.z - feet.z);
        if state.grounded {
            // Standing on top, not on the floor below.
            if feet.y > height - LEDGE_TOLERANCE && toward.z > -ON_LEDGE {
                return true;
            }
            // Back on the floor short of the top: it tries again for as long
            // as a walk leg would.
            if flown && tick - *short.get_or_insert(tick) > (RETRY_SECONDS * TICKS) as usize {
                return false;
            }
        } else {
            flown = true;
        }
        let pressing = toward.length() < crate::route::PRESS_NEAR;
        let jump = state.grounded && pressing;
        let crouch = crawl && (pressing && !state.grounded || feet.y > height - LEDGE_TOLERANCE);
        step(
            &mut player,
            &mut physics,
            toward.normalize_or_zero(),
            Press {
                jump,
                crouch,
                jet: false,
            },
        );
    }
    false
}

/// Whether a body with `tuning` walking crouched, with no jump, gets onto a
/// ledge `height` high with a roof over it as low as the route grid takes
/// for a crawlspace: the crouched body's height.
fn crawls_onto(tuning: &PlayerTuning, height: f32) -> bool {
    let edge = tuning.forward.max(1.0);
    let top = Vec3::new(0.0, height, -edge - ON_LEDGE);
    let far = Vec3::new(50.0, height, -edge);
    let near = Vec3::new(-50.0, -1.0, -edge - 50.0);
    let roof = height + tuning.crouch_height + LEDGE_TOLERANCE;
    let boxes = [
        (near, far),
        (
            Vec3::new(near.x, roof, near.z),
            Vec3::new(far.x, roof + 50.0, far.z),
        ),
    ];
    let mut physics = world(&boxes);
    let Some(mut player) = stand(&mut physics, tuning) else {
        return false;
    };
    for _ in 0..(MAX_SECONDS * TICKS) as usize {
        let state = player.state();
        let feet = Vec3::from(state.feet);
        let toward = Vec3::new(0.0, 0.0, top.z - feet.z);
        if state.grounded && feet.y > height - LEDGE_TOLERANCE && toward.z > -ON_LEDGE {
            return true;
        }
        step(
            &mut player,
            &mut physics,
            toward.normalize_or_zero(),
            Press {
                crouch: true,
                ..Press::default()
            },
        );
    }
    false
}

/// The highest crawlspace floor a body with `tuning` walks up into
/// crouched, no higher than its step.
fn crawl_step(tuning: &PlayerTuning) -> f32 {
    let (mut low, mut high) = (0.0, tuning.step_height);
    if crawls_onto(tuning, high) {
        return high;
    }
    while high - low > LEDGE_TOLERANCE {
        let mid = (low + high) * 0.5;
        if crawls_onto(tuning, mid) {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

/// The highest ledge a body with `tuning` jumps onto from a walk, between
/// what it steps up without jumping and what its jump speed could lift it to
/// with a step on top.
/// With `crawl`, the highest crawlspace floor a jump then a crouch in the
/// air gets it into.
fn ledge(tuning: &PlayerTuning, crawl: bool) -> f32 {
    let rise = tuning.jump_speed * tuning.jump_speed / (2.0 * tuning.gravity);
    let (mut low, mut high) = (tuning.step_height, rise + tuning.step_height);
    if !lands_on(tuning, low, crawl) {
        return tuning.step_height;
    }
    while high - low > LEDGE_TOLERANCE {
        let mid = (low + high) * 0.5;
        if lands_on(tuning, mid, crawl) {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

/// Where a body shoved off its feet comes down, and the impact it lands
/// with: the real player motor flown from `feet` at `velocity` with no
/// input, against the fixed collision of `ground` (its map, loaded terrain
/// tiles and liquids) the flight could reach. `tuning` is the target's
/// current one, its scale applied, so the body is the right size.
///
/// The impact is the hardest of the motor's own `MotionEvents::impact`s
/// from the shove until it lands (a wall slammed on the way down, or the
/// landing), each the vector the session hands its fall rule. `allowance`
/// is the caller's share of its planning budget: each collider copied for
/// the flight and each motor tick spends one. Not landed by the time that
/// runs out (a long drop, a shove off into nothing), its impact is what it
/// will land with at least: the hardest so far, or how fast it is falling
/// by then when there is something below to come down on, since that only
/// grows while it falls on (water deep enough to float it has slowed it
/// already). `None` when it cannot be flown at all.
///
/// Not covered: the target steering in the air (it is flown with no input),
/// other bodies and vehicles in the way, and openings it would pass through.
pub fn shove_landing(
    tuning: &PlayerTuning,
    feet: Vec3,
    velocity: Vec3,
    ground: &crate::nav::Ground,
    allowance: &mut u32,
) -> Option<Shoved> {
    // As far as the flight can go before the allowance runs out: its own
    // speed and gravity, with no input to add to either.
    let seconds = *allowance as f32 * FIXED_DT;
    let flat = Vec3::new(velocity.x, 0.0, velocity.z).length() * seconds + tuning.width;
    let rise = velocity.y.max(0.0) * seconds + tuning.stand_height;
    let fall = (-velocity.y).max(0.0) * seconds + 0.5 * tuning.gravity * seconds * seconds;
    let reach = Aabb::new(
        Vector::from_array((feet - Vec3::new(flat, fall, flat)).to_array()),
        Vector::from_array((feet + Vec3::new(flat, rise, flat)).to_array()),
    );
    let mut physics = bri_physics::new_world();
    for (_, collider) in ground
        .physics
        .query_pipeline_with_filter(QueryFilter::only_fixed().exclude_sensors())
        .intersect_aabb_conservative(reach)
    {
        *allowance = allowance.checked_sub(1)?;
        physics.insert_collider(
            ColliderBuilder::new(collider.shared_shape().clone())
                .position(*collider.position())
                .collision_groups(collider.collision_groups())
                .user_data(collider.user_data),
            None,
        );
    }
    bri_physics::detect_collisions(&mut physics);
    let mut player = Player::spawn_overlapping(&mut physics, 1, feet, tuning.clone()).ok()?;
    player.set_motion(velocity, false);
    let mut hardest = Vec3::ZERO;
    while *allowance > 0 {
        *allowance -= 1;
        let events = player
            .step_in_water(&mut physics, MoveInput::default(), ground.waters)
            .ok()?;
        physics.step();
        if events.impact.length() > hardest.length() {
            hardest = events.impact;
        }
        if events.landed {
            return Some(Shoved {
                landed: Some(Vec3::from(player.state().feet)),
                impact: hardest,
            });
        }
    }
    // Still falling with something below to come down on: at least that
    // hard. Into nothing it never lands.
    let falling = Vec3::from(player.state().velocity);
    let below = ground
        .ray(Vec3::from(player.state().feet), Vec3::NEG_Y, f32::MAX)
        .is_some();
    Some(Shoved {
        landed: None,
        impact: if below && falling.y < 0.0 && falling.length() > hardest.length() {
            falling
        } else {
            hardest
        },
    })
}

/// Where a shoved body comes down (`None`: not within the flight's
/// allowance) and the impact it lands with (`shove_landing`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shoved {
    pub landed: Option<Vec3>,
    pub impact: Vec3,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard() -> PlayerTuning {
        PlayerTuning::default()
    }

    fn shove(boxes: &[(Vec3, Vec3)], feet: Vec3, velocity: Vec3) -> (Option<Shoved>, u32) {
        let mut physics = bri_physics::new_world();
        for (min, max) in boxes {
            let half = (*max - *min) * 0.5;
            let centre = (*min + *max) * 0.5;
            physics.insert_collider(
                ColliderBuilder::cuboid(half.x, half.y, half.z)
                    .translation(Vector::new(centre.x, centre.y, centre.z)),
                None,
            );
        }
        bri_physics::detect_collisions(&mut physics);
        let passages = bri_content::passage::Passages {
            list: Vec::new(),
            closed: Vec::new(),
        };
        let ground = crate::nav::Ground {
            physics: &physics,
            terrain: &|_, _, _| None,
            passages: &passages,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        // Two seconds of flight.
        let mut allowance = TICKS as u32 * 2;
        let landing = shove_landing(&standard(), feet, velocity, &ground, &mut allowance);
        (landing, allowance)
    }

    /// Shoved off a ledge, a body comes down on the floor below it, past
    /// the edge, landing with a downward impact; shoved off into nothing it
    /// never lands, and the whole allowance is spent.
    #[test]
    fn a_shove_off_a_ledge_lands_below_and_one_into_nothing_does_not() {
        let ledge = (Vec3::new(-2.0, -1.0, -2.0), Vec3::new(2.0, 0.0, 2.0));
        let below = (Vec3::new(-50.0, -7.0, -50.0), Vec3::new(50.0, -6.0, 50.0));
        let (feet, push) = (Vec3::new(1.5, 0.0, 0.0), Vec3::new(8.0, 4.0, 0.0));
        let (landing, _) = shove(&[ledge, below], feet, push);
        let landing = landing.expect("it is flown");
        let (at, impact) = (
            landing.landed.expect("it lands on the floor below"),
            landing.impact,
        );
        assert!((at.y + 6.0).abs() < 0.1, "{at}");
        assert!(at.x > 2.0, "{at}");
        assert!(impact.y < 0.0, "{impact}");
        let (landing, left) = shove(&[ledge], feet, push);
        let landing = landing.expect("it is flown");
        assert_eq!(landing.landed, None);
        assert_eq!(left, 0);
        assert_eq!(landing.impact, Vec3::ZERO, "into nothing: no landing");
        // A floor far below, out of the allowance's reach: it lands at
        // least as hard as it is falling when that runs out.
        let deep = (
            Vec3::new(-50.0, -501.0, -50.0),
            Vec3::new(50.0, -500.0, 50.0),
        );
        let (landing, _) = shove(&[ledge, deep], feet, push);
        let landing = landing.expect("it is flown");
        assert_eq!(landing.landed, None);
        assert!(landing.impact.y < impact.y, "{} {impact}", landing.impact);
    }

    /// Shoved hard into a wall beside it, the impact it reports is the
    /// slam into the wall, not the soft landing after.
    #[test]
    fn a_shove_into_a_wall_reports_the_slam() {
        let floor = (Vec3::new(-50.0, -1.0, -50.0), Vec3::new(50.0, 0.0, 50.0));
        let wall = (Vec3::new(1.0, 0.0, -5.0), Vec3::new(2.0, 6.0, 5.0));
        let (landing, _) = shove(&[floor, wall], Vec3::ZERO, Vec3::new(12.0, 1.0, 0.0));
        let landing = landing.expect("it is flown");
        let (at, impact) = (
            landing.landed.expect("it lands on the floor"),
            landing.impact,
        );
        assert!(at.x < 1.0 && at.y.abs() < 0.1, "{at}");
        assert!(impact.x > impact.y.abs(), "{impact}");
    }
    /// A leg eight up onto a landing just beside its takeoff.
    fn climb(reach: &Reach) -> Flight {
        reach.jets.as_ref().expect("it jets").flight(8.0, 0.0)
    }

    /// Across open air a body lands on what its leaps were measured to
    /// reach and nothing farther; a landing a bot jumps onto from standing
    /// still is one it also lands on arriving at a walk.
    #[test]
    fn leaps_land_within_their_measured_reach_and_no_farther() {
        let t = standard();
        let leaps = &Reach::of(&t).leaps;
        // Level across a gap a unit wide: it lands.
        let gap = landing_width(&t) + 1.0;
        assert!(leaps.seconds(0.0, gap).is_some(), "{leaps:?}");
        // Down across one, farther.
        assert!(leaps.seconds(-2.0, gap + 1.0).is_some(), "{leaps:?}");
        // Every measured leap lands, as the bot flies it.
        for (i, row) in leaps.rows.iter().enumerate() {
            for (across, _) in row {
                assert!(leap(&t, leaps.rises[i], *across).is_some());
            }
            // One spacing past the farthest, it falls.
            if let Some((last, _)) = row.last() {
                assert!(
                    leap(&t, leaps.rises[i], last + LEAP_SPACING).is_none(),
                    "rise {}: {last}",
                    leaps.rises[i]
                );
            }
        }
        // Higher than its jump, nothing.
        assert_eq!(leaps.seconds(Reach::of(&t).ledge + 1.0, gap), None);
    }

    #[test]
    fn lower_gravity_leaps_farther() {
        let t = standard();
        let light = PlayerTuning {
            gravity: t.gravity * 0.5,
            ..t.clone()
        };
        let (heavy, light) = (Reach::measure(&t).leaps, Reach::measure(&light).leaps);
        assert!(
            light.farthest() > heavy.farthest(),
            "{} {}",
            light.farthest(),
            heavy.farthest()
        );
    }

    #[test]
    fn a_tuning_is_measured_once_and_shared() {
        let t = standard();
        assert!(Arc::ptr_eq(&Reach::of(&t), &Reach::of(&t.clone())));
    }

    #[test]
    fn the_standard_body_jumps_onto_what_its_jump_lifts_it_to() {
        let t = standard();
        let reach = Reach::of(&t);
        let rise = t.jump_speed * t.jump_speed / (2.0 * t.gravity);
        assert!(reach.ledge > t.step_height && reach.ledge < rise + t.step_height);
        assert!(lands_on(&t, reach.ledge, false));
        assert!(!lands_on(&t, reach.ledge + LEDGE_TOLERANCE * 2.0, false));
        // Into a crawlspace no higher than onto a ledge; the roof can only
        // cut the jump short.
        assert!(reach.crawl_ledge > t.step_height && reach.crawl_ledge <= reach.ledge);
        assert!(lands_on(&t, reach.crawl_ledge, true));
        assert!(!lands_on(
            &t,
            reach.crawl_ledge + LEDGE_TOLERANCE * 2.0,
            true
        ));
    }

    #[test]
    fn lower_gravity_jumps_higher_and_climbs_on_less_energy() {
        let t = standard();
        let light = PlayerTuning {
            gravity: t.gravity * 0.5,
            ..t.clone()
        };
        let (heavy, light) = (Reach::measure(&t), Reach::measure(&light));
        assert!(light.ledge > heavy.ledge, "{} {}", light.ledge, heavy.ledge);
        // A tall climb, where holding its weight up is most of the jetting.
        let tall = |r: &Reach| r.jets.as_ref().expect("it jets").flight(16.0, 0.0).jetting;
        assert!(tall(&light) < tall(&heavy));
    }

    #[test]
    fn a_stronger_jump_lands_on_higher_ledges() {
        let t = standard();
        let springy = PlayerTuning {
            jump_speed: t.jump_speed * 1.25,
            ..t.clone()
        };
        assert!(Reach::measure(&springy).ledge > Reach::measure(&t).ledge);
    }

    #[test]
    fn stronger_jets_fly_quicker_and_none_that_cannot_lift_it() {
        let t = standard();
        let strong = PlayerTuning {
            jet_acceleration: t.jet_acceleration * 1.5,
            ..t.clone()
        };
        let (strong, standard) = (climb(&Reach::measure(&strong)), climb(&Reach::measure(&t)));
        assert!(strong.seconds < standard.seconds && strong.jetting < standard.jetting);
        let weak = PlayerTuning {
            jet_acceleration: t.gravity * 0.9,
            ..t.clone()
        };
        assert!(Reach::measure(&weak).jets.is_none());
    }

    /// Crouched under a roof it walks up a step no higher than its own,
    /// and no higher than it was measured to.
    #[test]
    fn a_crouched_body_walks_up_into_a_crawlspace_as_high_as_measured() {
        let t = standard();
        let reach = Reach::of(&t);
        eprintln!("crawl step {} (step {})", reach.crawl_step, t.step_height);
        assert!(reach.crawl_step > 0.0 && reach.crawl_step <= t.step_height);
        assert!(crawls_onto(&t, reach.crawl_step));
        if reach.crawl_step < t.step_height {
            assert!(!crawls_onto(&t, reach.crawl_step + LEDGE_TOLERANCE * 2.0));
        }
    }
}
