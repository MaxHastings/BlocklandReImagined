//! The bot route planner's physics: what each way of getting about costs a
//! body, and the ordinary controls that carry a body along each leg
//! (`docs/architecture/bots.md`, Routes).
//!
//! The search itself is the walk grid's ([`crate::nav::Search`]): its edges
//! are walking, stepping, jumping, crawling, portals, swimming and jetting,
//! each costed here from the body's own tuning, so one search finds one
//! plan whose waypoints say which leg they belong to ([`Mode`]). A driver
//! follows a chassis plan from the same search with [`drive`].
//!
//! Nothing here knows any content: a body jets because its tuning can lift
//! it, swims because water floats it, and a vehicle turns as tightly as its
//! wheelbase and steering angle let it.
use crate::nav::Mode;
use crate::reach::{Handling, Reach};
use bri_motor::player::PlayerTuning;
use glam::Vec3;

/// Viscosity of stock still water (`WaterBlock`'s default), for a swim
/// speed the search can use before it knows which water it crosses.
const STOCK_VISCOSITY: f32 = 40.0;
/// Fixed cost of a takeoff, in walking units: a jet leg must save more
/// than a short detour before it is worth leaving the ground.
const TAKEOFF: f32 = 3.0;
/// Fixed cost of going into deep water, in walking units.
const WADE_IN: f32 = 1.5;

/// The parts of a body's tuning that say how long its moves take: its
/// speeds on foot, its jump and gravity, and its jets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub forward: f32,
    pub crouch_forward: f32,
    pub jump_speed: f32,
    pub gravity: f32,
    pub jet_acceleration: f32,
    pub can_jet: bool,
    /// How hard it steers in the air: its ground acceleration times its
    /// air control.
    pub air_acceleration: f32,
    /// How hard it speeds up and slows down on its feet.
    pub acceleration: f32,
}
impl Motion {
    pub fn of(tuning: &PlayerTuning) -> Self {
        Self {
            forward: tuning.forward,
            crouch_forward: tuning.crouch_forward,
            jump_speed: tuning.jump_speed,
            gravity: tuning.gravity,
            jet_acceleration: tuning.jet_acceleration,
            can_jet: tuning.can_jet,
            air_acceleration: tuning.acceleration * tuning.air_control,
            acceleration: tuning.acceleration,
        }
    }
    /// Which way to steer in the air to come down at `target` from `feet`
    /// moving at `velocity`: toward it, until it would carry on past it
    /// even braking from now on; then against its drift, so it stops over
    /// it. How a body jumping onto a small landing (a peg, a tread) comes
    /// down on it rather than past it.
    pub fn air_steer(&self, feet: Vec3, velocity: Vec3, target: Vec3) -> Vec3 {
        let (offset, drift) = (flat(target - feet), flat(velocity));
        let distance = offset.length();
        let along = if distance > f32::EPSILON {
            drift.dot(offset / distance)
        } else {
            drift.length()
        };
        let stopping = along * along / (2.0 * self.air_acceleration.max(f32::EPSILON));
        if along > 0.0 && stopping >= distance {
            -drift.normalize_or_zero()
        } else {
            offset.normalize_or_zero()
        }
    }
    /// Seconds a hop is in the air until it comes down `rise` above where
    /// it left the ground (below, for a negative `rise`): up at
    /// `jump_speed`, its jets (when it fires them) pushing it straight up
    /// for `jets` seconds more, then back down under `gravity`. The one
    /// flight model: a hop's landing (`session::bots`) and a jump's time on
    /// a route (`nav`). A rise above the top of the hop is reached there.
    pub fn hop(&self, jets: f32, rise: f32) -> f32 {
        let g = self.gravity.max(f32::EPSILON);
        let jets = if self.can_jet { jets.max(0.0) } else { 0.0 };
        // Under thrust it climbs at `jet_acceleration` less gravity.
        let thrust = self.jet_acceleration - g;
        let up = self.jump_speed + thrust * jets;
        let height = self.jump_speed * jets + 0.5 * thrust * jets * jets;
        let top = height + up.max(0.0).powi(2) / (2.0 * g);
        jets + up.max(0.0) / g + (2.0 * (top - rise).max(0.0) / g).sqrt()
    }
    /// Seconds it takes to stop from a walk.
    pub fn stop(&self) -> f32 {
        self.forward / self.acceleration.max(f32::EPSILON)
    }
    /// How high a jump lifts it.
    pub fn apex(&self) -> f32 {
        self.jump_speed * self.jump_speed / (2.0 * self.gravity.max(f32::EPSILON))
    }
    /// Seconds a body takes to fall `depth` from standing still.
    pub fn fall(&self, depth: f32) -> f32 {
        (2.0 * depth.max(0.0) / self.gravity.max(f32::EPSILON)).sqrt()
    }
}
/// How near a jump or crawl waypoint a walking body presses jump or crouch.
pub const PRESS_NEAR: f32 = 1.6;
/// Height over the landing a jet leg crosses at.
pub const JET_CLEARANCE: f32 = 1.0;

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// What a body's jets can do now: its measured jet legs
/// ([`crate::reach::JetReach`]) and the energy it holds for them.
#[derive(Clone, Debug, PartialEq)]
pub struct Jets {
    /// Jet legs as its motor flies them.
    pub reach: std::sync::Arc<Reach>,
    /// Seconds of jetting its energy holds now (no limit without a drain).
    pub seconds: f32,
    /// Its walking speed: seconds become walking units.
    pub walk_speed: f32,
    /// The kind's `fly` weight: above 1 it jets more readily, below less.
    pub weight: f32,
}
impl Jets {
    /// The jets of a body with `tuning` holding `energy`, for a kind with
    /// `weight` on flying; `None` when they do not lift it, it will not
    /// fly, or its energy does not hold even the shortest jet leg.
    pub fn of(tuning: &PlayerTuning, energy: f32, weight: f32) -> Option<Self> {
        if weight <= 0.0 || !weight.is_finite() {
            return None;
        }
        let reach = Reach::of(tuning);
        let shortest = reach.jets.as_ref()?.shortest();
        let seconds = if tuning.jet_drain > 0.0 {
            (energy - tuning.min_jet_energy).max(0.0) / tuning.jet_drain
        } else if energy >= tuning.min_jet_energy {
            f32::INFINITY
        } else {
            0.0
        };
        (seconds >= shortest.jetting).then_some(Self {
            reach,
            seconds,
            walk_speed: tuning.forward.max(1.0),
            weight,
        })
    }
    /// Seconds a jet leg takes from feet at `from` onto a landing at `to`,
    /// crossing [`JET_CLEARANCE`] above it, as measured; `None` when it
    /// climbs higher than its jets were measured to or its energy does not
    /// last.
    pub fn flight(&self, from: Vec3, to: Vec3) -> Option<f32> {
        let jets = self.reach.jets.as_ref()?;
        let up = to.y - from.y;
        if jets.highest().is_some_and(|highest| up > highest) {
            return None;
        }
        let flight = jets.flight(up, flat(to - from).length());
        (flight.jetting <= self.seconds).then_some(flight.seconds)
    }
    /// What a flight of `seconds` costs, in seconds of walking: at least
    /// walking the straight distance, so the search's estimate stays a
    /// lower bound.
    pub fn cost(&self, seconds: f32, from: Vec3, to: Vec3) -> f32 {
        let floor = flat(to - from).length() + (to.y - from.y).abs() * 0.5;
        (seconds / self.weight + TAKEOFF / self.walk_speed).max(floor / self.walk_speed)
    }
}

/// What deep water costs a body: each unit swum costs this many walked
/// (the swim's top speed against the walk's), and going in costs
/// [`Swim::entry`] more.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swim {
    pub per_unit: f32,
    pub entry: f32,
}
impl Swim {
    /// From the body's tuning: water drag (`drag` times viscosity) holds a
    /// swimmer's push (`swim_acceleration`) to a top speed, never above its
    /// underwater speed.
    pub fn of(tuning: &PlayerTuning) -> Self {
        let drag = (tuning.drag * STOCK_VISCOSITY).max(0.01);
        let speed = (tuning.swim_acceleration / drag)
            .min(tuning.underwater_forward)
            .max(0.5);
        Self {
            per_unit: (tuning.forward.max(1.0) / speed).max(1.0),
            entry: WADE_IN,
        }
    }
}
impl Default for Swim {
    fn default() -> Self {
        Self::of(&PlayerTuning::default())
    }
}

/// How a search may move a body besides walking: the edge costs.
#[derive(Clone, Debug, PartialEq)]
pub struct Costs {
    /// `None`: no swim legs; deep water's bottom is walked as if dry (a
    /// kind that keeps to its water swims there by itself).
    pub swim: Option<Swim>,
    /// `None`: no jet legs.
    pub jets: Option<Jets>,
}
impl Default for Costs {
    fn default() -> Self {
        Self {
            swim: Some(Swim::default()),
            jets: None,
        }
    }
}

/// Whether a walk leg is getting anywhere: its net displacement over a
/// window, not its speed at an instant. A body hopping against what blocks
/// it, or wobbling back and forth between two spots, moves every tick and
/// goes nowhere. The window and the distance come from the body's own
/// walking speed: over [`Progress::WINDOW`] seconds a body on its way
/// covers at least a fifth of what it walks in that time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Progress {
    /// Where the window started, and the tick; none between legs.
    from: Option<(Vec3, u64)>,
    /// Windows in a row gone nowhere.
    stalls: u32,
}
impl Progress {
    /// Seconds a window lasts.
    pub const WINDOW: f32 = 0.75;
    /// Share of a window's walk that counts as getting somewhere.
    const SHARE: f32 = 0.2;
    /// Ends the window: the body is not on a walk leg now.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    /// Windows in a row that went nowhere, the latest included.
    pub fn stalls(&self) -> u32 {
        self.stalls
    }
    /// One tick on a walk leg at `tick`, the body at `feet` walking at up
    /// to `walk_speed` (units a second) and `hz` ticks a second: true once a
    /// window closes with less than its share of a walk made good across.
    pub fn stalled(&mut self, feet: Vec3, walk_speed: f32, hz: f32, tick: u64) -> bool {
        let window = (Self::WINDOW * hz).ceil() as u64;
        let Some((from, since)) = self.from else {
            self.from = Some((feet, tick));
            return false;
        };
        if tick < since + window {
            return false;
        }
        self.from = Some((feet, tick));
        let across = Vec3::new(feet.x - from.x, 0.0, feet.z - from.z).length();
        let stalled = across < walk_speed * Self::WINDOW * Self::SHARE;
        self.stalls = if stalled { self.stalls + 1 } else { 0 };
        stalled
    }
}

/// Headway on a pursuit: its best reading so far (a height climbed, a spot
/// it stood at, a distance to its point) and the tick it last beat it by
/// enough to count. Every give-up judge that asks "has this got anywhere
/// lately" (a jet climb, a driven chassis, an objective approach, a claim's
/// lease) reads one of these; what counts as beating the mark is the
/// caller's (`note`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Headway<T> {
    mark: Option<(T, u64)>,
}
impl<T: Copy> Headway<T> {
    /// Headway marked at `reading` from `tick`.
    pub fn from(reading: T, tick: u64) -> Self {
        Self {
            mark: Some((reading, tick)),
        }
    }
    /// A reading at `tick`: marked, and true, when there is no mark yet or
    /// `beats(mark, reading)`.
    pub fn note(&mut self, reading: T, tick: u64, beats: impl FnOnce(T, T) -> bool) -> bool {
        let ahead = self.mark.is_none_or(|(mark, _)| beats(mark, reading));
        if ahead {
            self.mark = Some((reading, tick));
        }
        ahead
    }
    /// Marks `reading` at `tick` whatever it is: a fresh start.
    pub fn mark(&mut self, reading: T, tick: u64) {
        self.mark = Some((reading, tick));
    }
    /// Keeps the mark's reading and starts its clock again at `tick`.
    pub fn restart(&mut self, tick: u64) {
        if let Some((_, since)) = self.mark.as_mut() {
            *since = tick;
        }
    }
    /// No mark: the next reading starts one.
    pub fn clear(&mut self) {
        self.mark = None;
    }
    /// Ticks since the mark (none without one).
    pub fn idle(&self, tick: u64) -> u64 {
        self.mark.map_or(0, |(_, since)| tick.saturating_sub(since))
    }
}

/// Where a jet leg is: set when it starts, kept while its waypoint leads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JetLeg {
    /// The waypoint it flies to, and the tick it started.
    pub to: Vec3,
    pub since: u64,
    /// Highest it has been: it has climbed to its crossing height.
    pub crossing: bool,
    /// Where it lifts off: the middle of the cell the leg was planned from.
    pub from: Vec3,
    /// The highest it has climbed so far, and when it got there.
    pub risen: Headway<f32>,
}
impl JetLeg {
    /// A leg to `to` lifting off at `from`, started at `tick`.
    pub fn start(to: Vec3, from: Vec3, tick: u64) -> Self {
        Self {
            to,
            since: tick,
            crossing: false,
            from,
            risen: Headway::from(from.y, tick),
        }
    }
    /// Whether what really happened says to give the leg up at `tick`: it
    /// took longer than planned allows (`seconds`), its climb stopped
    /// rising (something it could not see from the grid is over it), or it
    /// came down off the landing's height after climbing: below it, or on
    /// something standing above it (another body).
    pub fn failed(
        &mut self,
        feet: Vec3,
        grounded: bool,
        step: f32,
        seconds: f32,
        tick: u64,
    ) -> bool {
        self.risen.note(feet.y, tick, |high, y| y > high + 0.2);
        tick.saturating_sub(self.since) > patience(seconds)
            || !self.crossing && self.risen.idle(tick) > CLIMB_STALL
            || grounded && self.crossing && (feet.y - self.to.y).abs() > step + 0.5
    }
}
/// Ticks a climb may go without rising before the leg is given up.
const CLIMB_STALL: u64 = 90;

/// The controls of one tick of a jet leg.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JetControl {
    /// Flat direction to move in (zero: none).
    pub direction: Vec3,
    pub jet: bool,
    pub jump: bool,
}

/// Flat speed below which a body counts as standing still.
const STILL: f32 = 0.6;
/// How near the middle of the cell it was planned from a jet leg lifts off.
pub const TAKEOFF_TOLERANCE: f32 = crate::nav::CELL * 0.5;
/// How far under its crossing height a jet leg counts as up there.
const CROSSING_BAND: f32 = 0.3;

/// One tick of a jet leg to `to` crossing at `apex`, for a body with
/// `tuning`: stand still where it takes off, climb straight up there (jets
/// lift hardest with no move, and let go once it will coast the rest),
/// cross at the crossing height (climbing again whenever it sinks below
/// it, as a moving jet does), and over the landing let the jets go and
/// brake onto it. The leg's measured times ([`crate::reach`]) are this
/// controller's own, flown from a standing takeoff.
pub fn jet(
    leg: &mut JetLeg,
    feet: Vec3,
    velocity: Vec3,
    grounded: bool,
    to: Vec3,
    apex: f32,
    tuning: &PlayerTuning,
) -> JetControl {
    let toward = flat(to - feet);
    let across = toward.length();
    let toward = toward.normalize_or_zero();
    let closing = flat(velocity).dot(toward);
    if feet.y >= apex - CROSSING_BAND {
        leg.crossing = true;
    }
    let drift = flat(velocity);
    // Over the landing: no more thrust, only braking onto it.
    if across < 0.9 && feet.y > to.y - 0.3 {
        return JetControl {
            direction: if drift.length() > STILL {
                -drift.normalize()
            } else {
                Vec3::ZERO
            },
            jet: false,
            jump: false,
        };
    }
    // Not yet standing still where it lifts off: get there and stop first,
    // or the climb carries it on under whatever it is climbing beside.
    let off = flat(leg.from - feet);
    if grounded && !leg.crossing && (off.length() > TAKEOFF_TOLERANCE || drift.length() > STILL) {
        // As fast as it can still stop from before the spot: no faster
        // than it walks, and nothing once there (the motor brakes).
        let speed = if off.length() > TAKEOFF_TOLERANCE {
            (2.0 * tuning.acceleration * off.length())
                .sqrt()
                .min(tuning.forward)
        } else {
            0.0
        };
        return JetControl {
            direction: off.normalize_or_zero() * speed / tuning.forward.max(f32::EPSILON),
            jet: false,
            jump: false,
        };
    }
    // Rising fast enough to coast up to the crossing height: let go, or it
    // overshoots and spends the time coming back down.
    let coast = |speed: f32| speed.max(0.0).powi(2) / (2.0 * tuning.gravity);
    let up = apex - CROSSING_BAND - feet.y;
    if !grounded && !leg.crossing && coast(velocity.y) >= up {
        return JetControl::default();
    }
    // Not yet up at the crossing height, or sinking below it (a moving jet
    // lifts less than it weighs): climb.
    if feet.y < apex - CROSSING_BAND || velocity.y < 0.0 && feet.y < apex {
        return JetControl {
            direction: Vec3::ZERO,
            jet: true,
            // A jump starts the climb, unless it alone would carry it past.
            jump: grounded && coast(tuning.jump_speed) <= up,
        };
    }
    // Cross: thrust toward the landing until going as fast as it can still
    // slow down from before it, then against the motion.
    let wanted = (leg_push_speed(across)).min(10.0);
    let direction = if closing > wanted { -toward } else { toward };
    JetControl {
        direction,
        // Up to the crossing height: a body whose moving jets lift more
        // than it weighs would otherwise climb on as it crosses.
        jet: feet.y < apex,
        jump: false,
    }
}

/// A leap of a route under way: from a standstill at its takeoff, a jump
/// across open air onto a landing ([`crate::reach::Leaps`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeapLeg {
    /// The landing it leaps onto, and where it takes off.
    pub to: Vec3,
    pub from: Vec3,
    /// The tick the leg started, and the seconds it allows for walking to
    /// the takeoff and stopping there.
    pub since: u64,
    approach: f32,
    /// The tick it jumped.
    pub jumped: Option<u64>,
    /// It has left the ground on the jump.
    pub flown: bool,
}
impl LeapLeg {
    /// A leg onto `to` from `from` for a body at `feet` with `tuning`,
    /// started at `tick`.
    pub fn start(to: Vec3, from: Vec3, feet: Vec3, tuning: &PlayerTuning, tick: u64) -> Self {
        let speed = tuning.forward.max(f32::EPSILON);
        Self {
            to,
            from,
            since: tick,
            // Walking there, then braking from a walk to a stop.
            approach: flat(from - feet).length() / speed + Motion::of(tuning).stop(),
            jumped: None,
            flown: false,
        }
    }
    /// Whether what really happened says to give the leg up at `tick`: it
    /// took longer than the walk to the takeoff or the measured leap
    /// (`seconds`) allow, or it came down after the jump anywhere but on
    /// the landing's height.
    pub fn failed(&self, feet: Vec3, grounded: bool, step: f32, seconds: f32, tick: u64) -> bool {
        let late = match self.jumped {
            Some(at) => tick.saturating_sub(at) > patience(seconds),
            None => tick.saturating_sub(self.since) > patience(self.approach),
        };
        late || grounded && self.flown && (feet.y - self.to.y).abs() > step + 0.5
    }
}

/// One tick of a leap leg for a body with `tuning`: come down on the
/// takeoff (still in the air from the move before), stand still on it, as
/// the leap was measured from ([`crate::reach::Leaps`]), jump, and steer in
/// the air to come down on the landing ([`Motion::air_steer`]).
pub fn leap(
    leg: &mut LeapLeg,
    feet: Vec3,
    velocity: Vec3,
    grounded: bool,
    tuning: &PlayerTuning,
    tick: u64,
) -> JetControl {
    let motion = Motion::of(tuning);
    if leg.jumped.is_none() {
        if !grounded {
            return JetControl {
                direction: motion.air_steer(feet, velocity, leg.from),
                ..JetControl::default()
            };
        }
        let off = flat(leg.from - feet);
        if off.length() > TAKEOFF_TOLERANCE || flat(velocity).length() > STILL {
            // To the takeoff, as fast as it can still stop there.
            let speed = if off.length() > TAKEOFF_TOLERANCE {
                (2.0 * tuning.acceleration * off.length())
                    .sqrt()
                    .min(tuning.forward)
            } else {
                0.0
            };
            return JetControl {
                direction: off.normalize_or_zero() * speed / tuning.forward.max(f32::EPSILON),
                ..JetControl::default()
            };
        }
        leg.jumped = Some(tick);
    }
    if !grounded {
        leg.flown = true;
    }
    JetControl {
        direction: if leg.flown {
            motion.air_steer(feet, velocity, leg.to)
        } else {
            flat(leg.to - feet).normalize_or_zero()
        },
        // Held until it leaves the ground.
        jump: !leg.flown,
        jet: false,
    }
}

/// A crossing speed a jet's push can still stop from within `across`.
fn leg_push_speed(across: f32) -> f32 {
    (2.0 * 6.0 * across).sqrt()
}

/// How many times its measured time a leg through the air may take before it has gone
/// wrong: something the bare measuring floor did not have (wind from a
/// blast, a body in the way) is holding it up.
const PATIENCE: f32 = 2.0;

/// How long a leg through the air of `seconds` (as measured) may take
/// before the bot gives it up and plans again from where it is.
pub fn patience(seconds: f32) -> u64 {
    (seconds * PATIENCE / bri_physics::FIXED_DT) as u64
}

/// Which way a driver drives toward its next point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gear {
    #[default]
    Forward,
    /// Backing up. `nose`: steering so the nose swings toward a point ahead
    /// that lies inside its turning circle, until it comes out of it;
    /// otherwise backing rear first onto a point behind.
    Reverse { nose: bool },
    /// Pulling straight ahead, away from a point behind that lies inside
    /// its turning circle, until it comes out of it.
    PullOut,
}
impl Gear {
    /// Its travel sign (1 forward, -1 back) and the hull heading change it
    /// steers for, from `error`, the point's bearing off the nose.
    pub fn steer(self, error: f32) -> (f32, f32) {
        use std::f32::consts::{PI, TAU};
        match self {
            Gear::Forward => (1.0, error),
            Gear::PullOut => (1.0, 0.0),
            Gear::Reverse { nose: true } => (-1.0, error),
            Gear::Reverse { nose: false } => (-1.0, (error + TAU).rem_euclid(TAU) - PI),
        }
    }
}

/// What a driver steers by: its chassis's turn and speeds, and its kind's
/// reversing policy (`REVERSE_DEGREES`, `REVERSE_DISTANCE` in the bots' drive).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Driving {
    /// Its tightest turn (`reach::Handling::tightest`).
    pub radius: f32,
    /// The fastest it holds that turn, give or take `reach`
    /// (`reach::Handling::manoeuvre_speed`).
    pub manoeuvre: f32,
    /// How near a point it counts as there: half its width.
    pub reach: f32,
    /// Cruise speeds, forward and in reverse.
    pub cruise: (f32, f32),
    /// Radians off the nose past which a point is behind.
    pub behind: f32,
    /// The farthest a point behind is backed onto (a pursued target: a
    /// farther one is turned round for, not driven at as a long retreat).
    pub reverse_limit: f32,
}

/// The gear for a point `distance` away at `error` radians off the nose,
/// after `was`. Each side of the chassis has a turning circle; a point
/// deeper than `reach` inside one cannot be driven onto forward, only
/// circled. Then it opens the distance first: backs away from a point
/// ahead (the nose swinging toward it) or pulls ahead of one behind, until
/// the point is out of the circle. A point behind is backed onto rear
/// first, within `reverse_limit`, when that is sooner, at its `cruise`
/// speeds, than turning round for it at full lock. This is the one
/// reversing rule. The radius is the one it turns at manoeuvring speed: it
/// can always brake to that first, so a fast approach slows rather than
/// changing gear.
pub fn gear(drive: &Driving, error: f32, distance: f32, was: Gear) -> Gear {
    let Driving {
        radius,
        manoeuvre,
        reach,
        cruise,
        behind,
        reverse_limit,
    } = *drive;
    // The point relative to the centre of the circle on its side.
    let ahead = distance * error.cos();
    let sideways = distance * error.sin().abs();
    let from_centre = ahead.hypot(sideways - radius);
    let clear = if matches!(was, Gear::Reverse { nose: true } | Gear::PullOut) {
        radius
    } else {
        radius - reach
    };
    let inside = from_centre < clear;
    let behind = error.abs() > behind;
    match (inside, behind) {
        (true, false) => Gear::Reverse { nose: true },
        (true, true) => Gear::PullOut,
        (false, true)
            if distance <= reverse_limit
                && distance / cruise.1.max(0.1)
                    < std::f32::consts::PI * radius / manoeuvre.max(0.1)
                        + distance / cruise.0.max(0.1) =>
        {
            Gear::Reverse { nose: false }
        }
        _ => Gear::Forward,
    }
}

/// How fast a driver in `gear` may go toward a point `distance` away
/// whose bearing is `heading` off the way it steers: pure pursuit takes an
/// arc of distance^2 / (2 x sideways), and a point abeam or behind is
/// turned for at full lock; either no faster than it was measured holding
/// that turn (`Handling::arc_speed`), never slower than its manoeuvring
/// speed, and no faster than its `cruise` (forward, reverse). Backing or
/// pulling out of its turning circle goes at manoeuvring speed.
pub fn pace(handling: &Handling, drive: &Driving, gear: Gear, heading: f32, distance: f32) -> f32 {
    let (radius, cruise) = (drive.radius, drive.cruise);
    let arc = if heading.abs() >= std::f32::consts::FRAC_PI_2 {
        radius
    } else {
        distance * distance / (2.0 * (distance * heading.sin().abs()).max(1e-3))
    };
    match gear {
        Gear::Forward => handling.arc_speed(arc).max(drive.manoeuvre).min(cruise.0),
        Gear::Reverse { nose: false } => handling.arc_speed(arc).max(drive.manoeuvre).min(cruise.1),
        Gear::Reverse { nose: true } | Gear::PullOut => drive.manoeuvre.min(cruise.1),
    }
}

/// Whether a hull at `rotation` (x, y, z, w) still stands on its wheels:
/// its up within 60 degrees of the world's.
pub fn upright(rotation: [f32; 4]) -> bool {
    (glam::Quat::from_array(rotation) * Vec3::Y).y >= 0.5
}

/// Seconds getting into a seat takes once beside it.
pub const BOARD_SECONDS: f32 = 1.0;
/// The most throttle a driver gives: it eases off from there for its
/// heading error (`reach::Handling` is measured at it).
pub const DRIVE_THROTTLE: f32 = 0.8;
/// The share of its top speed a driver cruises at.
pub const CRUISE: f32 = 0.6;

/// Whether a board-and-drive leg serves a goal `walk` away on foot at
/// `walk_speed`: walking `to_seat` to the seat, boarding and driving
/// `drive` at `cruise` gets there sooner than walking straight there.
pub fn drive_serves(walk_speed: f32, walk: f32, to_seat: f32, drive: f32, cruise: f32) -> bool {
    let walk_speed = walk_speed.max(0.1);
    to_seat / walk_speed + BOARD_SECONDS + drive / cruise.max(0.1) < walk / walk_speed
}

/// Whether a walker at `walk_speed` closes on a target `toward` it (flat,
/// from the walker) moving at `velocity`: the target draws away slower
/// than the walker walks, with a tenth to spare, so a runner as fast as
/// the walker is not left to an on-foot chase that never closes.
pub fn walk_closes(walk_speed: f32, toward: Vec3, velocity: Vec3) -> bool {
    let away = Vec3::new(toward.x, 0.0, toward.z).normalize_or_zero();
    velocity.dot(away) < walk_speed * 0.9
}

/// The label of a waypoint's leg, for diagnostics.
pub fn leg_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Walk => "walk",
        Mode::Swim => "swim",
        Mode::Jet { .. } => "jet",
        Mode::Leap { .. } => "leap",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The standard player.
    static G: std::sync::LazyLock<PlayerTuning> = std::sync::LazyLock::new(PlayerTuning::default);

    #[test]
    fn a_body_wobbling_between_two_spots_is_stalled_within_a_window() {
        let (walk, hz) = (7.0, 120.0);
        let window = (Progress::WINDOW * hz).ceil() as u64;
        // Back and forth a unit and a half, every tick moving.
        let mut wobble = Progress::default();
        let stalled = (0..=window * 2).find(|&t| {
            let x = if (t / 20).is_multiple_of(2) { 0.0 } else { 1.5 };
            wobble.stalled(Vec3::new(x, 0.0, 0.0), walk, hz, t)
        });
        assert!(
            stalled.is_some_and(|t| t <= window),
            "stalled at {stalled:?}"
        );
        assert_eq!(wobble.stalls(), 1);
        // Still going nowhere: each window counts, until it gets somewhere.
        assert!((window + 1..=window * 2 + 1).any(|t| wobble.stalled(Vec3::ZERO, walk, hz, t)));
        assert_eq!(wobble.stalls(), 2);
        wobble.stalled(Vec3::new(9.0, 0.0, 0.0), walk, hz, window * 4);
        assert_eq!(wobble.stalls(), 0);
        // Hopping in place: up and down is not across.
        let mut hop = Progress::default();
        assert!((0..=window).any(|t| hop.stalled(
            Vec3::new(0.0, (t % 30) as f32 * 0.05, 0.0),
            walk,
            hz,
            t
        )));
        // Walking at a third of its speed round a corner gets somewhere.
        let mut walking = Progress::default();
        for t in 0..window * 6 {
            let d = t as f32 / hz * walk / 3.0;
            let at = if d < 4.0 {
                Vec3::new(d, 0.0, 0.0)
            } else {
                Vec3::new(4.0, 0.0, d - 4.0)
            };
            assert!(!walking.stalled(at, walk, hz, t), "walking stalled at {t}");
        }
    }

    #[test]
    fn a_walker_closes_only_on_a_target_drawing_away_slower_than_it_walks() {
        let toward = Vec3::new(3.0, 0.0, 4.0);
        let away = toward.normalize();
        assert!(walk_closes(7.0, toward, Vec3::ZERO));
        assert!(walk_closes(7.0, toward, -away * 7.0), "coming closer");
        assert!(walk_closes(7.0, toward, away * 3.0), "slower than a walk");
        assert!(!walk_closes(7.0, toward, away * 7.0), "as fast as a walk");
        // Sideways is not away.
        assert!(walk_closes(7.0, toward, Vec3::new(-4.0, 0.0, 3.0) * 3.0));
    }

    #[test]
    fn a_driver_backs_out_of_its_circle_and_turns_no_faster_than_measured() {
        let car = Handling::measure(&crate::reach::test_car(|_| {}), 1.0).expect("it drives");
        let r = car.tightest;
        // Straight ahead: drive. Close beside: inside the circle, back up.
        let reach = 1.0;
        let drive = Driving {
            radius: r,
            manoeuvre: car.manoeuvre_speed(reach),
            reach,
            cruise: (18.0, 6.0),
            behind: 103f32.to_radians(),
            reverse_limit: f32::INFINITY,
        };
        let gear = |error, distance, was| gear(&drive, error, distance, was);
        // A pursued target behind is backed onto only within the limit.
        let pursuing = Driving {
            reverse_limit: 1.5,
            ..drive
        };
        assert_eq!(
            super::gear(&drive, 3.1, 2.0, Gear::Forward),
            Gear::Reverse { nose: false }
        );
        assert_eq!(
            super::gear(&pursuing, 3.1, 2.0, Gear::Forward),
            Gear::Forward
        );
        assert_eq!(gear(0.1, 10.0, Gear::Forward), Gear::Forward);
        let beside = std::f32::consts::FRAC_PI_2;
        assert_eq!(
            gear(beside, 2.0, Gear::Forward),
            Gear::Reverse { nose: true }
        );
        // Far beside: a turn reaches it.
        assert_eq!(gear(beside, 12.0, Gear::Forward), Gear::Forward);
        // Close straight behind: back onto it; far behind: turn round.
        assert_eq!(gear(3.1, 2.0, Gear::Forward), Gear::Reverse { nose: false });
        assert_eq!(gear(3.1, 40.0, Gear::Forward), Gear::Forward);
        // Straight ahead goes at cruise; a turn at lock, slower; backing
        // out, at manoeuvring speed.
        let cruise = drive.cruise;
        let pace = |gear, heading, distance| pace(&car, &drive, gear, heading, distance);
        assert_eq!(pace(Gear::Forward, 0.0, 300.0), cruise.0);
        let turning = pace(Gear::Forward, 3.0, 10.0);
        assert!((turning - drive.manoeuvre).abs() < 1e-4, "{turning}");
        assert!(turning < cruise.0, "a turn at lock is slower");
        assert!(pace(Gear::PullOut, 0.0, 2.0) <= cruise.1);
        // Close behind to one side, inside the circle: pull ahead first.
        assert_eq!(gear(2.2, 2.5, Gear::Forward), Gear::PullOut);
        // A full-lock turn that passes within reach of it is good enough.
        assert_eq!(gear(beside, 2.0 * r - 0.5, Gear::Forward), Gear::Forward);
        // Backing out holds until it is out of the circle: no flip-flop.
        assert_eq!(
            gear(beside, 2.0 * r - 0.5, Gear::Reverse { nose: true }),
            Gear::Reverse { nose: true }
        );
        let (sign, heading) = Gear::Reverse { nose: false }.steer(3.0);
        assert_eq!(sign, -1.0);
        assert!((heading - (3.0 - std::f32::consts::PI)).abs() < 1e-4);
    }

    #[test]
    fn a_hull_on_its_side_or_roof_is_not_upright() {
        assert!(upright(glam::Quat::IDENTITY.to_array()));
        assert!(upright(glam::Quat::from_rotation_x(0.9).to_array()));
        assert!(!upright(glam::Quat::from_rotation_z(1.2).to_array()));
        assert!(!upright(
            glam::Quat::from_rotation_x(std::f32::consts::PI).to_array()
        ));
    }

    #[test]
    fn a_vehicle_serves_only_a_goal_it_reaches_sooner_than_walking() {
        // A jeep beside the bot and an enemy 60 away: drive.
        assert!(drive_serves(6.0, 60.0, 2.0, 58.0, 18.0));
        // The enemy a few steps off: walking is sooner.
        assert!(!drive_serves(6.0, 6.0, 2.0, 6.0, 18.0));
        // A jeep far back the other way: not worth the detour.
        assert!(!drive_serves(6.0, 30.0, 25.0, 50.0, 18.0));
    }

    #[test]
    fn standard_jets_fly_measured_legs() {
        let t = PlayerTuning::default();
        let jets = Jets::of(&t, t.max_energy, 1.0).expect("the standard player jets");
        // Eight units up beside where it stands takes a few seconds.
        let up = jets.flight(Vec3::ZERO, Vec3::new(0.0, 8.0, 2.0)).unwrap();
        assert!((1.0..6.0).contains(&up), "{up}");
        // Higher and farther take longer.
        assert!(jets.flight(Vec3::ZERO, Vec3::new(0.0, 16.0, 2.0)).unwrap() > up);
        assert!(jets.flight(Vec3::ZERO, Vec3::new(0.0, 8.0, 20.0)).unwrap() > up);
        // A kind that never flies, or a body that cannot lift itself, has none.
        assert!(Jets::of(&t, t.max_energy, 0.0).is_none());
        let heavy = PlayerTuning {
            jet_acceleration: t.gravity,
            ..t.clone()
        };
        assert!(Jets::of(&heavy, 100.0, 1.0).is_none());
        let no_jets = PlayerTuning {
            can_jet: false,
            ..t.clone()
        };
        assert!(Jets::of(&no_jets, 100.0, 1.0).is_none());
    }

    #[test]
    fn a_flight_needs_the_energy_it_drains() {
        let t = PlayerTuning {
            jet_drain: 20.0,
            ..PlayerTuning::default()
        };
        let jets = |energy: f32| Jets::of(&t, energy, 1.0);
        let full = jets(t.max_energy).unwrap();
        let (low, high) = (Vec3::new(0.0, 3.0, 2.0), Vec3::new(0.0, 15.0, 2.0));
        let needs = |to: Vec3| {
            let r = full.reach.jets.as_ref().unwrap();
            r.flight(to.y, flat(to).length()).jetting * t.jet_drain
        };
        assert!(needs(low) < needs(high));
        // Energy for the low leg and not the high one flies only the low one.
        let between = jets((needs(low) + needs(high)) * 0.5).unwrap();
        assert!(between.flight(Vec3::ZERO, low).is_some());
        assert!(between.flight(Vec3::ZERO, high).is_none());
        // Too little for even the shortest leg: no jets at all.
        assert!(jets(needs(low) * 0.5).is_none());
    }

    #[test]
    fn swimming_costs_more_than_walking_the_same_distance() {
        let swim = Swim::default();
        assert!(swim.per_unit > 1.0 && swim.per_unit < 4.0, "{swim:?}");
    }

    #[test]
    fn a_jet_leg_climbs_first_then_crosses_then_lets_go_over_the_landing() {
        let mut leg = JetLeg::start(Vec3::new(4.0, 8.0, 0.0), Vec3::ZERO, 0);
        let to = Vec3::new(4.0, 8.0, 0.0);
        let c = jet(&mut leg, Vec3::ZERO, Vec3::ZERO, true, to, 9.0, &G);
        assert!(c.jet && c.jump && c.direction == Vec3::ZERO, "{c:?}");
        let c = jet(
            &mut leg,
            Vec3::new(0.0, 8.9, 0.0),
            Vec3::ZERO,
            false,
            to,
            9.0,
            &G,
        );
        assert!(c.jet && c.direction.x > 0.9, "crossing: {c:?}");
        let c = jet(
            &mut leg,
            Vec3::new(3.6, 8.6, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            false,
            to,
            9.0,
            &G,
        );
        assert!(!c.jet && c.direction.x < 0.0, "landing brakes: {c:?}");
        // Sunk below the lip on the way over: climb again.
        let c = jet(
            &mut leg,
            Vec3::new(2.0, 8.1, 0.0),
            Vec3::ZERO,
            false,
            to,
            9.0,
            &G,
        );
        assert!(c.jet && c.direction == Vec3::ZERO, "{c:?}");
        // Up at the crossing height but sinking, as a moving jet does: climb.
        let c = jet(
            &mut leg,
            Vec3::new(2.0, 8.9, 0.0),
            Vec3::new(3.0, -1.0, 0.0),
            false,
            to,
            9.0,
            &G,
        );
        assert!(c.jet && c.direction == Vec3::ZERO, "{c:?}");
    }

    #[test]
    fn a_jet_leg_stops_running_before_it_takes_off() {
        let to = Vec3::new(4.0, 8.0, 0.0);
        let mut leg = JetLeg::start(to, Vec3::ZERO, 0);
        // Running on past it: let go and let the motor brake.
        let c = jet(
            &mut leg,
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 5.0),
            true,
            to,
            9.0,
            &G,
        );
        assert!(!c.jet && !c.jump && c.direction == Vec3::ZERO, "{c:?}");
        // A step past it: back to it.
        let c = jet(
            &mut leg,
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::ZERO,
            true,
            to,
            9.0,
            &G,
        );
        assert!(!c.jet && !c.jump && c.direction.z < 0.0, "{c:?}");
        let c = jet(&mut leg, Vec3::ZERO, Vec3::ZERO, true, to, 9.0, &G);
        assert!(c.jet && c.jump, "{c:?}");
    }

    #[test]
    fn a_jet_leg_that_comes_down_on_something_above_its_landing_fails() {
        let to = Vec3::new(16.0, 6.0, 30.0);
        let mut leg = JetLeg::start(to, Vec3::new(10.0, 2.0, 30.0), 0);
        let _ = jet(
            &mut leg,
            Vec3::new(12.0, 9.0, 30.0),
            Vec3::ZERO,
            false,
            to,
            9.0,
            &G,
        );
        assert!(leg.crossing);
        // Standing on another body's head over the landing.
        assert!(leg.failed(Vec3::new(16.3, 8.66, 30.0), true, 0.5, 3.0, 60));
        // Standing on the landing itself is not a failure.
        assert!(!leg.failed(Vec3::new(16.0, 6.0, 30.0), true, 0.5, 3.0, 61));
    }
}
