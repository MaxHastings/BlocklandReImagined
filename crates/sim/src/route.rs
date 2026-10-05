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
/// Farthest across and up a single jet leg reaches.
pub const JET_RANGE: f32 = 30.0;
pub const JET_CLIMB: f32 = 32.0;
/// Height over the landing a jet leg crosses at.
pub const JET_CLEARANCE: f32 = 1.0;

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// What a body's jets can do now, from its tuning and energy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Jets {
    /// Net upward acceleration jetting with no move: thrust less gravity.
    pub climb: f32,
    /// Flat acceleration jetting with a move (thrust leans into the move).
    pub push: f32,
    /// How fast it sinks jetting with a move: gravity less the thrust's lift.
    pub sink: f32,
    /// Seconds of jetting its energy holds now (no limit without a drain).
    pub seconds: f32,
    /// Its walking speed: seconds become walking units.
    pub walk_speed: f32,
    /// The kind's `fly` weight: above 1 it jets more readily, below less.
    pub weight: f32,
}
impl Jets {
    /// The jets of a body with `tuning` holding `energy`, for a kind with
    /// `weight` on flying; `None` when it cannot lift itself or will not.
    pub fn of(tuning: &PlayerTuning, energy: f32, weight: f32) -> Option<Self> {
        if !tuning.can_jet || weight <= 0.0 || !weight.is_finite() {
            return None;
        }
        let climb = tuning.jet_acceleration - tuning.gravity;
        if climb < 0.25 || energy < tuning.min_jet_energy {
            return None;
        }
        let lean = (1.0 + tuning.jet_lift * tuning.jet_lift).sqrt();
        let seconds = if tuning.jet_drain > 0.0 {
            (energy - tuning.min_jet_energy).max(0.0) / tuning.jet_drain
        } else {
            f32::INFINITY
        };
        (seconds >= 0.5).then_some(Self {
            climb,
            push: tuning.jet_acceleration / lean,
            sink: (tuning.gravity - tuning.jet_acceleration * tuning.jet_lift / lean).max(0.0),
            seconds,
            walk_speed: tuning.forward.max(1.0),
            weight,
        })
    }
    /// Seconds of jetting from feet at `from` up to `apex`, across and down
    /// onto `to`, if its energy lasts: climbing with no move, then crossing
    /// with the move (climbing back whatever the crossing sinks).
    pub fn flight(&self, from: Vec3, to: Vec3, apex: f32) -> Option<f32> {
        let up = (apex - from.y).max(0.0);
        let across = flat(to - from).length();
        if across > JET_RANGE || up > JET_CLIMB {
            return None;
        }
        let climb = (2.0 * up / self.climb).sqrt();
        // Speed up half way, slow down the rest.
        let cross = 2.0 * (across / self.push).sqrt();
        let sunk = 0.25 * self.sink * cross * cross;
        let reclimb = (2.0 * sunk / self.climb).sqrt();
        let jetting = climb + cross + reclimb;
        (jetting <= self.seconds).then_some(jetting + 0.5)
    }
    /// What a flight of `seconds` costs, in walking units: at least the
    /// straight distance, so the search's estimate stays a lower bound.
    pub fn cost(&self, seconds: f32, from: Vec3, to: Vec3) -> f32 {
        let floor = flat(to - from).length() + (to.y - from.y).abs() * 0.5;
        (seconds * self.walk_speed / self.weight + TAKEOFF).max(floor)
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
#[derive(Clone, Copy, Debug, PartialEq)]
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

/// Where a jet leg is: set when it starts, kept while its waypoint leads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JetLeg {
    /// The waypoint it flies to, and the tick it started.
    pub to: Vec3,
    pub since: u64,
    /// Highest it has been: it has climbed to its crossing height.
    pub crossing: bool,
    /// Where it took off.
    pub from: Vec3,
    /// The highest it has climbed so far, and when it got there.
    pub risen: (f32, u64),
}
impl JetLeg {
    /// A leg to `to` starting from `feet` at `tick`.
    pub fn start(to: Vec3, feet: Vec3, tick: u64) -> Self {
        Self {
            to,
            since: tick,
            crossing: false,
            from: feet,
            risen: (feet.y, tick),
        }
    }
    /// Whether what really happened says to give the leg up at `tick`: it
    /// took longer than planned allows (`seconds`), its climb stopped
    /// rising (something it could not see from the grid is over it), or it
    /// came down below the landing after climbing.
    pub fn failed(
        &mut self,
        feet: Vec3,
        grounded: bool,
        step: f32,
        seconds: f32,
        tick: u64,
    ) -> bool {
        if feet.y > self.risen.0 + 0.2 {
            self.risen = (feet.y, tick);
        }
        tick.saturating_sub(self.since) > jet_patience(seconds)
            || !self.crossing && tick.saturating_sub(self.risen.1) > CLIMB_STALL
            || grounded && self.crossing && feet.y < self.to.y - step - 0.5
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

/// One tick of a jet leg to `to` crossing at `apex`: climb straight up
/// where it took off (jets lift hardest with no move), cross at the
/// crossing height (climbing again whenever it sinks to the landing's
/// lip), and over the landing let the jets go and brake onto it.
pub fn jet(
    leg: &mut JetLeg,
    feet: Vec3,
    velocity: Vec3,
    grounded: bool,
    to: Vec3,
    apex: f32,
) -> JetControl {
    let toward = flat(to - feet);
    let across = toward.length();
    let toward = toward.normalize_or_zero();
    let closing = flat(velocity).dot(toward);
    if feet.y >= apex - 0.3 {
        leg.crossing = true;
    }
    // Over the landing: no more thrust, only braking onto it.
    if across < 0.9 && feet.y > to.y - 0.3 {
        let drift = flat(velocity);
        return JetControl {
            direction: if drift.length() > 0.6 {
                -drift.normalize()
            } else {
                Vec3::ZERO
            },
            jet: false,
            jump: false,
        };
    }
    // Below the landing's lip, or not yet up at the crossing height: climb.
    if !leg.crossing || feet.y < to.y + 0.4 {
        return JetControl {
            direction: Vec3::ZERO,
            jet: true,
            jump: grounded,
        };
    }
    // Cross: thrust toward the landing until going as fast as it can still
    // slow down from before it, then against the motion.
    let wanted = (leg_push_speed(across)).min(10.0);
    let direction = if closing > wanted { -toward } else { toward };
    JetControl {
        direction,
        jet: feet.y < apex || velocity.y < -1.5,
        jump: false,
    }
}

/// A crossing speed a jet's push can still stop from within `across`.
fn leg_push_speed(across: f32) -> f32 {
    (2.0 * 6.0 * across).sqrt()
}

/// How long a jet leg of `seconds` (as planned) may take before the bot
/// gives it up and plans again from where it is.
pub fn jet_patience(seconds: f32) -> u64 {
    ((seconds * 2.0 + 2.0) * 120.0) as u64
}

/// Sideways acceleration a chassis's tyres hold in a turn, widening its
/// turning circle with speed.
const CORNERING: f32 = 10.0;

/// How a wheeled chassis turns: from its definition's wheels and steering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chassis {
    /// Distance between its frontmost and rearmost axles.
    pub wheelbase: f32,
    /// Steering angle at full lock (`max_steering`), radians.
    pub steer: f32,
    /// How far the frontmost and rearmost wheels turn with the steering
    /// (`Wheel::steering`: 1 a steered front, 0 a fixed rear, below 0 a
    /// rear that steers against it).
    pub front: f32,
    pub rear: f32,
}
impl Chassis {
    /// From its wheels as (position along the chassis, scaled, where -z is
    /// forward; steering share), its full-lock angle and its length.
    pub fn of(wheels: impl IntoIterator<Item = (f32, f32)>, steer: f32, length: f32) -> Self {
        let wheels: Vec<(f32, f32)> = wheels.into_iter().collect();
        let front = wheels
            .iter()
            .copied()
            .reduce(|a, b| if b.0 < a.0 { b } else { a });
        let rear = wheels
            .iter()
            .copied()
            .reduce(|a, b| if b.0 > a.0 { b } else { a });
        match (front, rear) {
            (Some(f), Some(r)) if r.0 - f.0 > 0.1 => Self {
                wheelbase: r.0 - f.0,
                steer,
                front: f.1,
                rear: r.1,
            },
            _ => Self {
                wheelbase: (length * 0.6).max(0.5),
                steer,
                front: 1.0,
                rear: 0.0,
            },
        }
    }
    /// The radius of its tightest turn at walking pace, set by its
    /// wheelbase and lock.
    pub fn radius(&self) -> f32 {
        let turn = (self.steer * self.front).tan() - (self.steer * self.rear).tan();
        self.wheelbase / turn.abs().max(0.05)
    }
    /// The fastest it can take an arc of `radius` before its tyres let go.
    pub fn arc_speed(radius: f32) -> f32 {
        (CORNERING * radius.max(0.0)).sqrt()
    }
    /// The fastest it still turns as tightly as it can: the speed for
    /// backing out of or pulling out of its circle.
    pub fn manoeuvre_speed(&self) -> f32 {
        Self::arc_speed(self.radius())
    }
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

/// The gear for a point `distance` away at `error` radians off the nose,
/// after `was`, for a chassis whose tightest turn has `radius` and that
/// arrives within `reach` of a point. Each side of it has a turning
/// circle; a point deeper than `reach` inside one cannot be driven onto
/// forward, only circled. Then it opens the distance first: backs away
/// from a point ahead (the nose swinging toward it) or pulls ahead of one
/// behind, until the point is out of the circle. A point behind is backed
/// onto rear first when that is sooner, at its `cruise` (forward, reverse)
/// speeds, than turning round for it at full lock. The radius is the one
/// it turns at manoeuvring speed: it can always brake to that first, so a
/// fast approach slows rather than changing gear.
pub fn gear(
    radius: f32,
    reach: f32,
    cruise: (f32, f32),
    error: f32,
    distance: f32,
    was: Gear,
) -> Gear {
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
    let behind = error.abs() > 1.8;
    match (inside, behind) {
        (true, false) => Gear::Reverse { nose: true },
        (true, true) => Gear::PullOut,
        (false, true)
            if distance / cruise.1.max(0.1)
                < std::f32::consts::PI * radius / Chassis::arc_speed(radius)
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
/// turned for at full lock; either no faster than its tyres hold, nor than
/// its `cruise` (forward, reverse). Backing or pulling out of its turning
/// circle goes at manoeuvring speed.
pub fn pace(radius: f32, gear: Gear, heading: f32, distance: f32, cruise: (f32, f32)) -> f32 {
    let arc = if heading.abs() >= std::f32::consts::FRAC_PI_2 {
        radius
    } else {
        distance * distance / (2.0 * (distance * heading.sin().abs()).max(1e-3))
    };
    match gear {
        Gear::Forward => Chassis::arc_speed(arc.max(radius)).min(cruise.0),
        Gear::Reverse { nose: false } => Chassis::arc_speed(arc.max(radius)).min(cruise.1),
        Gear::Reverse { nose: true } | Gear::PullOut => Chassis::arc_speed(radius).min(cruise.1),
    }
}

/// Seconds getting into a seat takes once beside it.
pub const BOARD_SECONDS: f32 = 1.0;
/// The share of its top speed a driver cruises at.
pub const CRUISE: f32 = 0.6;

/// Whether a board-and-drive leg serves a goal `walk` away on foot at
/// `walk_speed`: walking `to_seat` to the seat, boarding and driving
/// `drive` at `cruise` gets there sooner than walking straight there.
pub fn drive_serves(walk_speed: f32, walk: f32, to_seat: f32, drive: f32, cruise: f32) -> bool {
    let walk_speed = walk_speed.max(0.1);
    to_seat / walk_speed + BOARD_SECONDS + drive / cruise.max(0.1) < walk / walk_speed
}

/// The label of a waypoint's leg, for diagnostics.
pub fn leg_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Walk => "walk",
        Mode::Swim => "swim",
        Mode::Jet { .. } => "jet",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chassis_turns_by_its_wheelbase_and_lock_and_backs_out_of_its_circle() {
        // The test car: wheels 1.6 ahead and behind, front steered, 0.8 lock.
        let car = Chassis::of([(-1.6, 1.0), (-1.6, 1.0), (1.6, 0.0), (1.6, 0.0)], 0.8, 4.8);
        let r = car.radius();
        assert!((r - 3.2 / 0.8f32.tan()).abs() < 1e-3, "{r}");
        assert!(
            Chassis::arc_speed(2.0 * r) > car.manoeuvre_speed(),
            "wider arcs take more speed"
        );
        // Rear wheels steering against the front turn tighter.
        let four = Chassis::of([(-1.8, 1.0), (1.8, -0.5)], 0.8, 5.0);
        assert!(four.radius() < 3.6 / 0.8f32.tan());
        // Straight ahead: drive. Close beside: inside the circle, back up.
        let reach = 1.0;
        let gear = |error, distance, was| gear(r, reach, (18.0, 6.0), error, distance, was);
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
        assert_eq!(pace(r, Gear::Forward, 0.0, 30.0, (18.0, 6.0)), 18.0);
        let turning = pace(r, Gear::Forward, 3.0, 10.0, (18.0, 6.0));
        assert!((turning - car.manoeuvre_speed()).abs() < 1e-4, "{turning}");
        assert!(pace(r, Gear::PullOut, 0.0, 2.0, (18.0, 6.0)) <= 6.0);
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
    fn a_vehicle_serves_only_a_goal_it_reaches_sooner_than_walking() {
        // A jeep beside the bot and an enemy 60 away: drive.
        assert!(drive_serves(6.0, 60.0, 2.0, 58.0, 18.0));
        // The enemy a few steps off: walking is sooner.
        assert!(!drive_serves(6.0, 6.0, 2.0, 6.0, 18.0));
        // A jeep far back the other way: not worth the detour.
        assert!(!drive_serves(6.0, 30.0, 25.0, 50.0, 18.0));
    }

    #[test]
    fn standard_jets_climb_slowly_and_cross_fast() {
        let t = PlayerTuning::default();
        let jets = Jets::of(&t, t.max_energy, 1.0).expect("the standard player jets");
        assert!(jets.climb > 1.0 && jets.climb < 4.0, "{jets:?}");
        assert!(jets.push > 15.0, "{jets:?}");
        assert!(jets.sink > 0.0, "a moving jet sinks: {jets:?}");
        // Eight units straight up takes a few seconds.
        let up = jets
            .flight(Vec3::ZERO, Vec3::new(0.0, 8.0, 0.5), 9.0)
            .unwrap();
        assert!((2.0..6.0).contains(&up), "{up}");
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
        let jets = Jets::of(&t, 100.0, 1.0).unwrap();
        assert!(
            jets.flight(Vec3::ZERO, Vec3::new(1.0, 2.0, 0.0), 3.0)
                .is_some()
        );
        assert!(
            jets.flight(Vec3::ZERO, Vec3::new(1.0, 30.0, 0.0), 31.0)
                .is_none()
        );
        assert!(
            Jets::of(&t, 5.0, 1.0).is_none(),
            "a quarter second of jetting"
        );
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
        let c = jet(&mut leg, Vec3::ZERO, Vec3::ZERO, true, to, 9.0);
        assert!(c.jet && c.jump && c.direction == Vec3::ZERO, "{c:?}");
        let c = jet(
            &mut leg,
            Vec3::new(0.0, 8.9, 0.0),
            Vec3::ZERO,
            false,
            to,
            9.0,
        );
        assert!(c.jet && c.direction.x > 0.9, "crossing: {c:?}");
        let c = jet(
            &mut leg,
            Vec3::new(3.6, 8.6, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            false,
            to,
            9.0,
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
        );
        assert!(c.jet && c.direction == Vec3::ZERO, "{c:?}");
    }
}
