//! Where a player body can walk, and paths across it, for bots.
//!
//! The ground is sampled on demand on a half-unit grid (one brick stud),
//! only where a bot's search reaches, from the world's fixed collision:
//! bricks' chunk colliders, the map and terrain. Moving bodies (players,
//! vehicles, items) are left out; a bot that walks into one is stuck for a
//! moment and plans again. Each sample remembers the floor it found. When
//! bricks change, `Nav::invalidate` forgets only the samples their boxes
//! touch, so a busy build server keeps almost all of its grid.
//!
//! A search runs a bounded number of new samples per call and resumes on
//! the next, so a long path never stalls a tick: bots spend at most
//! [`SAMPLES_PER_TICK`] new samples and [`EXPANSIONS_PER_TICK`] node
//! expansions between them each tick.
//!
//! A gap a body only fits crouched (a crawlspace) is walkable too: its
//! waypoints say to crouch, and its cells cost more, so a path crawls only
//! where walking upright is the long way round.
//!
//! The openings of linked bricks (portals) are links in the grid: a step
//! whose body middle goes in through one, as the motor carries a body, leads
//! to the cell it comes out at by the partner, so paths lead through portals
//! wherever walking through one is the way.
//!
//! The grid is the bot route planner's graph (`docs/architecture/bots.md`,
//! Routes): besides walking, a floor under water deep enough to float the
//! body is a swim cell, and a body with jets ([`crate::route::Jets`]) may
//! fly from any cell with open sky to the goal's floor. Each waypoint says
//! which of those legs it belongs to ([`Mode`]); [`crate::route`] costs them
//! from the body's tuning and turns each leg into controls.
use crate::route::Costs;
use bri_content::passage::Passages;
use bri_content::water::Water;
use glam::Vec3;
use rapier3d::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Grid spacing, in world units: one brick stud.
pub const CELL: f32 = 0.5;
/// Moves the grid avoids (`Nav::avoid`) it keeps at most before
/// forgetting the lapsed ones.
const MAX_AVOIDED: usize = 4096;
/// Farthest (cells) a jet leg's landing moves off a goal someone stands on.
const LANDING_RING: i32 = 4;
/// Horizontal distance within which a grid node completes its search.
pub(crate) const ARRIVAL_RADIUS: f32 = CELL * 1.5;
/// New ground samples all bots together may take in one tick.
pub const SAMPLES_PER_TICK: u32 = 96;
/// Nodes all searches together expand in one tick, remembered ground or not.
pub const EXPANSIONS_PER_TICK: u32 = 384;
/// New leap arcs all searches together sweep in one tick, each a handful
/// of shape casts ([`Ground::sweep_arc`]).
pub const ARCS_PER_TICK: u32 = 24;
/// Nodes one search expands before it settles for the closest it reached.
pub const MAX_EXPANSIONS: u32 = 6000;
/// Remembered samples before the whole cache starts over.
const MAX_SAMPLES: usize = 1 << 18;
/// Height quantum of a sample's source hint.
const HINT_BAND: f32 = 0.25;

/// What the grid is sampled for: a standing player body and what its motor
/// can climb, jump and drop.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub width: f32,
    pub height: f32,
    /// Height crouched: what fits through a crawlspace.
    pub crouch_height: f32,
    /// Highest ledge it walks up without jumping.
    pub step: f32,
    /// Highest ledge a jump lands it on.
    pub jump: f32,
    /// Highest crawlspace floor a jump then a crouch in the air gets it into.
    pub crawl_jump: f32,
    /// Deepest drop it walks off.
    pub drop: f32,
    /// Cosine of the steepest floor it stands on.
    pub floor_cos: f32,
    /// Full footprint for a chassis; pedestrians retain grid alignment slack.
    pub conservative: bool,
    /// Lower hull clearance above the support plane (wheels are not walls).
    pub bottom: f32,
    /// Floats and swims in deep water (a player body); a chassis does not.
    pub swims: bool,
    /// How long its moves take: every move costs the seconds its own
    /// speeds, jump and gravity take ([`Search`]).
    pub motion: crate::route::Motion,
    /// Its jumps across open air, as its motor was measured to make them.
    pub leaps: std::sync::Arc<crate::reach::Leaps>,
}
impl Body {
    pub fn of(tuning: &bri_motor::player::PlayerTuning, scale: f32) -> Self {
        let reach = if scale == 1.0 {
            crate::reach::Reach::of(tuning)
        } else {
            crate::reach::Reach::of(&tuning.clone().scaled(scale))
        };
        Self {
            width: tuning.width * scale,
            height: tuning.stand_height * scale,
            crouch_height: tuning.crouch_height * scale,
            step: tuning.step_height,
            // As high as its own motor was measured to jump on.
            jump: reach.ledge.max(tuning.step_height),
            crawl_jump: reach.crawl_ledge.max(tuning.step_height),
            drop: 4.0,
            floor_cos: tuning.slope_degrees.to_radians().cos(),
            conservative: false,
            bottom: 0.0,
            swims: true,
            motion: crate::route::Motion::of(tuning),
            leaps: reach.leaps.clone(),
        }
    }
    /// Seconds a body walking `across` on a floor it stands on (crouched:
    /// `crawl`) takes.
    fn walk_seconds(&self, across: f32, crawl: bool) -> f32 {
        let speed = if crawl {
            self.motion.crouch_forward
        } else {
            self.motion.forward
        };
        across / speed.max(f32::EPSILON)
    }
    /// Seconds of walking a distance is: what the search counts in.
    fn seconds(&self, distance: f32) -> f32 {
        self.walk_seconds(distance, false)
    }
    /// The box a clearance test uses: narrower by one cell so a body that
    /// fits a gap passes whichever cell centre it is aligned to, and without
    /// the bottom half step, which the motor steps over.
    fn clearance(&self, crouched: bool) -> (f32, f32, f32) {
        let height = if crouched {
            self.crouch_height
        } else {
            self.height
        };
        let half_width = if self.conservative {
            self.width * 0.5
        } else {
            ((self.width - CELL) * 0.5).max(self.width * 0.25)
        };
        let lift = (self.step * 0.5)
            .min(height * 0.3)
            .max(self.bottom)
            .min(height - 0.01);
        (half_width, lift, height - lift)
    }
}

/// The fixed world the grid is sampled from.
pub struct Ground<'a> {
    pub physics: &'a PhysicsWorld,
    /// Exact terrain ray (origin, normalized direction, reach) giving the
    /// distance and normal, when the map has terrain.
    pub terrain: &'a dyn Fn(Vec3, Vec3, f32) -> Option<(f32, Vec3)>,
    /// The openings bodies pass through.
    pub passages: &'a Passages,
    /// Liquid volumes (map water and water bricks): deep water is swum.
    pub waters: &'a [Water],
    /// Boxes (min, max) of the moving bodies about (players): the grid
    /// leaves them out, but nobody takes off into one, and a pulled
    /// straight walk ([`pull`]) does not cut past one where the grid's own
    /// route keeps its lane.
    pub bodies: &'a [(Vec3, Vec3)],
    /// Each body's velocity, in `bodies`' order (none: all standing): a
    /// pulled walk also keeps clear of where one is going ([`MOTION_AHEAD`]).
    pub motions: &'a [Vec3],
}
/// How far ahead (seconds) a moving body's path counts as taken for a
/// pulled walk ([`Ground::crowds`]): two walkers heading into each other
/// keep their grid lanes instead of both cutting onto one line.
pub const MOTION_AHEAD: f32 = 1.0;
/// Share of a standing body under water from which it floats: the walk
/// grid's floor there is out of reach of its feet, and it swims.
const FLOATS: f32 = 0.6;
impl Ground<'_> {
    fn filter() -> QueryFilter<'static> {
        QueryFilter::only_fixed().exclude_sensors()
    }
    /// The surface of water that floats a body standing at `feet`, if any.
    pub fn floats(&self, body: &Body, feet: Vec3) -> Option<f32> {
        self.waters
            .iter()
            .filter(|w| w.coverage(feet.to_array(), body.height) >= FLOATS)
            .map(|w| w.max[1])
            .reduce(f32::max)
    }
    /// Whether a body at `feet` is in any water at all.
    fn wet(&self, body: &Body, feet: Vec3) -> bool {
        self.waters
            .iter()
            .any(|w| w.coverage(feet.to_array(), body.height) > 0.0)
    }
    /// Whether the standing body sweeps from `from` to `to` (feet) touching
    /// no fixed collision: a flight's climb, crossing or descent.
    pub fn sweep(&self, body: &Body, from: Vec3, to: Vec3) -> bool {
        let (half_width, _, _) = body.clearance(false);
        self.sweep_box(half_width, body.height, from, to)
    }
    /// Whether the standing body jumping from `from` comes down at `to`
    /// touching no fixed collision on the way: up at its jump speed and
    /// back down under gravity; across from a standstill, speeding up
    /// evenly over the first half of the flight and slowing evenly to a
    /// stop over the second, as it steers ([`crate::route::leap`]). Swept
    /// as straight legs that stray from the arc by no more than
    /// [`ARC_SAG`].
    pub fn sweep_arc(&self, body: &Body, from: Vec3, to: Vec3) -> bool {
        let motion = body.motion;
        let flight = motion.hop(0.0, to.y - from.y);
        let pieces = (flight * (motion.gravity / (8.0 * ARC_SAG)).sqrt())
            .ceil()
            .max(1.0) as usize;
        let at = |i: usize| {
            if i == pieces {
                return to;
            }
            let t = flight * i as f32 / pieces as f32;
            let y = from.y + motion.jump_speed * t - 0.5 * motion.gravity * t * t;
            let u = t / flight;
            let along = if u < 0.5 {
                2.0 * u * u
            } else {
                1.0 - 2.0 * (1.0 - u) * (1.0 - u)
            };
            (from + (to - from) * along).with_y(y)
        };
        (0..pieces).all(|i| self.sweep(body, at(i), at(i + 1)))
    }
    /// [`Ground::sweep`] for a box `half_width` across and `height` tall.
    pub fn sweep_box(&self, half_width: f32, height: f32, from: Vec3, to: Vec3) -> bool {
        let half = Vector::new(half_width, height * 0.5, half_width);
        let shape = Cuboid::new(half);
        let start = from + Vec3::Y * (height * 0.5 + 0.1);
        let pose = Pose::translation(start.x, start.y, start.z);
        let query = self.physics.query_pipeline_with_filter(Self::filter());
        if query.intersect_shape(pose, &shape).next().is_some() {
            return false;
        }
        query
            .cast_shape(
                &pose,
                Vector::from_array((to - from).to_array()),
                &shape,
                rapier3d::parry::query::ShapeCastOptions {
                    max_time_of_impact: 1.0,
                    stop_at_penetration: true,
                    ..Default::default()
                },
            )
            .is_none()
    }
    /// Whether the full-width standing body walks straight from `from` to
    /// `to` (feet): its box sweeps clear of fixed collision a step up, and
    /// all the way along a floor it can stand on lies within a step of the
    /// line, with no water deep enough to float it. What the grid's own
    /// steps check cell by cell, along one line.
    pub fn walkable(&self, body: &Body, from: Vec3, to: Vec3) -> bool {
        let rise = to.y - from.y;
        let across = Vec3::new(to.x - from.x, 0.0, to.z - from.z).length();
        if rise.abs() > body.step || across < 1e-3 {
            return across < 1e-3 && rise.abs() <= body.step;
        }
        let (_, lift, tall) = body.clearance(false);
        let half = Vector::new(body.width * 0.5, tall * 0.5, body.width * 0.5);
        let shape = Cuboid::new(half);
        let start = from + Vec3::Y * (lift + tall * 0.5);
        let pose = Pose::translation(start.x, start.y, start.z);
        // Loose bodies (a ball, a parked vehicle) count too: the grid
        // leaves them out, so a straight line must not cut through one.
        // Only players, who move out of the way, are left out.
        let query = self
            .physics
            .query_pipeline_with_filter(QueryFilter::exclude_kinematic().exclude_sensors());
        if query.intersect_shape(pose, &shape).next().is_some()
            || query
                .cast_shape(
                    &pose,
                    Vector::from_array((to - from).to_array()),
                    &shape,
                    rapier3d::parry::query::ShapeCastOptions {
                        max_time_of_impact: 1.0,
                        stop_at_penetration: true,
                        ..Default::default()
                    },
                )
                .is_some()
        {
            return false;
        }
        let samples = (across / (CELL * 0.5)).ceil() as usize;
        (1..samples).all(|i| {
            let at = from.lerp(to, i as f32 / samples as f32);
            self.stands(body, at) && self.floats(body, at).is_none()
        })
    }
    /// Whether a floor the body can stand on lies within a step of `at`.
    fn stands(&self, body: &Body, at: Vec3) -> bool {
        let top = at + Vec3::Y * (body.step + 0.05);
        self.ray(top, Vec3::NEG_Y, body.step * 2.0 + 0.1)
            .is_some_and(|(distance, normal)| {
                (top.y - distance - at.y).abs() <= body.step && normal.y >= body.floor_cos
            })
    }
    /// Where the full-width body stands at a grid node's `feet`: there, or
    /// pushed sideways out of what it overlaps by no more than the half
    /// cell the grid's narrower clearance box left it (`Body::clearance`),
    /// onto a floor it stands on. `None` when it fits nowhere in that slack.
    /// So a route through a gap only just wider than the body crosses where
    /// the whole body fits, not on the cell line beside it.
    pub fn settle(&self, body: &Body, feet: Vec3, crouched: bool) -> Option<Vec3> {
        let (_, lift, tall) = body.clearance(crouched);
        self.settle_box(body, body.width * 0.5, lift, tall, feet)
    }
    /// [`Ground::settle`] for a box `half_width` across, `tall` high from
    /// `lift` over the feet.
    fn settle_box(
        &self,
        body: &Body,
        half_width: f32,
        lift: f32,
        tall: f32,
        feet: Vec3,
    ) -> Option<Vec3> {
        let shape = SharedShape::cuboid(half_width, tall * 0.5, half_width);
        let query = self.physics.query_pipeline_with_filter(Self::filter());
        // Each push spends some of the half cell on each axis; when that
        // runs out, it fits nowhere near enough. Every push is at least
        // `SETTLE_GAP`, so this ends.
        let (mut at, mut spent) = (feet, Vec3::ZERO);
        loop {
            let pose = Pose::translation(at.x, at.y + lift + tall * 0.5, at.z);
            // The deepest overlap, as the way out and how far.
            let deepest = query
                .intersect_shape(pose, shape.as_ref())
                .filter_map(|(_, other)| {
                    rapier3d::parry::query::contact(
                        &pose,
                        shape.as_ref(),
                        other.position(),
                        other.shape(),
                        0.0,
                    )
                    .ok()
                    .flatten()
                })
                .map(|c| (-Vec3::from(c.normal1.to_array()), -c.dist))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            let Some((out, depth)) = deepest else {
                return self.stands(body, at).then_some(at);
            };
            let sideways = Vec3::new(out.x, 0.0, out.z);
            if sideways.length() < f32::EPSILON {
                return None;
            }
            let push = sideways.normalize() * (depth.max(0.0) / sideways.length() + SETTLE_GAP);
            spent += push.abs();
            if spent.max_element() > CELL * 0.5 {
                return None;
            }
            at += push;
        }
    }
    /// Whether a straight walk from `from` to `to` passes within reach of a
    /// moving body about (`bodies`), or of where it is going over the next
    /// [`MOTION_AHEAD`] seconds (`motions`): the body's half width plus the
    /// other's. Its own grid route is left to keep its lane there.
    pub fn crowds(&self, body: &Body, from: Vec3, to: Vec3) -> bool {
        let (_, _, tall) = body.clearance(false);
        self.bodies.iter().enumerate().any(|(i, (min, max))| {
            let centre = (*min + *max) * 0.5;
            if max.y < from.y.min(to.y) || min.y > from.y.max(to.y) + tall {
                return false;
            }
            let motion = self.motions.get(i).copied().unwrap_or(Vec3::ZERO);
            let going = centre + Vec3::new(motion.x, 0.0, motion.z) * MOTION_AHEAD;
            let reach = body.width * 0.5 + (max.x - min.x).max(max.z - min.z) * 0.5;
            segment_gap(from, to, centre, going) < reach
        })
    }
    /// How far above `origin` the underside over it is, within `reach`:
    /// `None` when there is none, or when `origin` is inside something (a
    /// wall the column rises through), which is no roof over it.
    fn ceiling(&self, origin: Vec3, reach: f32) -> Option<f32> {
        let ray = Ray::new(Vector::from_array(origin.to_array()), Vector::Y);
        let hit = self
            .physics
            .query_pipeline_with_filter(Self::filter())
            .cast_ray(&ray, reach, true)
            .map(|(_, distance)| distance);
        if hit.is_some_and(|d| d <= 0.0) {
            return None;
        }
        let terrain = (self.terrain)(origin, Vec3::Y, reach).map(|(d, _)| d);
        match (hit, terrain) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
    /// Whether `at` is inside fixed collision.
    fn inside(&self, at: Vec3) -> bool {
        self.physics
            .query_pipeline_with_filter(Self::filter())
            .intersect_point(Vector::from_array(at.to_array()))
            .next()
            .is_some()
    }
    /// The nearest surface along a ray and its normal.
    fn ray(&self, origin: Vec3, direction: Vec3, reach: f32) -> Option<(f32, Vec3)> {
        let ray = Ray::new(
            Vector::from_array(origin.to_array()),
            Vector::from_array(direction.to_array()),
        );
        let mut best = self
            .physics
            .query_pipeline_with_filter(Self::filter())
            .cast_ray_and_get_normal(&ray, reach, false)
            .map(|(_, hit)| (hit.time_of_impact, Vec3::from(hit.normal.to_array())));
        if let Some((distance, normal)) = (self.terrain)(origin, direction, reach)
            && best.is_none_or(|(d, _)| distance < d)
        {
            best = Some((distance, normal));
        }
        best
    }
    /// Where in the cell centred at `centre` a body stands (or crouches)
    /// clear of fixed collision on the floor there: the centre, or else
    /// pushed out of what it overlaps by no more than half a cell each way
    /// (bounded as [`Ground::settle`] is), still over a floor it stands on.
    fn place(&self, body: &Body, centre: Vec3, crouched: bool) -> Option<Vec3> {
        if self.clear(body, centre, crouched) {
            return Some(centre);
        }
        let (half_width, lift, tall) = body.clearance(crouched);
        self.settle_box(body, half_width, lift, tall, centre)
            .filter(|at| self.clear(body, *at, crouched) && (at.y - centre.y).abs() < f32::EPSILON)
    }
    /// Whether a body stands (or crouches) at `feet` without touching
    /// fixed collision, allowing for grid alignment.
    fn clear(&self, body: &Body, feet: Vec3, crouched: bool) -> bool {
        let (half_width, lift, tall) = body.clearance(crouched);
        self.empty(feet, half_width, lift, tall)
    }
    /// Whether the full-width body stands (or crouches) at `feet` touching
    /// nothing.
    fn fits(&self, body: &Body, feet: Vec3, crouched: bool) -> bool {
        let (_, lift, tall) = body.clearance(crouched);
        self.empty(feet, body.width * 0.5, lift, tall)
    }
    fn empty(&self, feet: Vec3, half_width: f32, lift: f32, tall: f32) -> bool {
        let shape = SharedShape::cuboid(half_width, tall * 0.5, half_width);
        let pose = Pose::translation(feet.x, feet.y + lift + tall * 0.5, feet.z);
        self.physics
            .query_pipeline_with_filter(Self::filter())
            .intersect_shape(pose, shape.as_ref())
            .next()
            .is_none()
    }
}

/// A grid node: a cell, the height of its floor, in tenths, and where in
/// the cell the body stands on it, in hundredths off the cell's centre.
/// A cell is a bucket, not a point: where only part of it holds the body
/// (overlapping stair treads, a ledge beside a wall), its node stands the
/// body there, and every edge, cost and waypoint uses that spot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Node {
    pub x: i32,
    pub z: i32,
    pub y: i32,
    ox: i8,
    oz: i8,
}
/// The unit of a node's spot off its cell's centre.
const SPOT_UNIT: f32 = 0.01;
impl Node {
    fn at(x: i32, z: i32, feet: f32) -> Self {
        Self {
            x,
            z,
            y: (feet * 10.0).round() as i32,
            ox: 0,
            oz: 0,
        }
    }
    /// The node of `floor` in cell `x, z`, standing where the floor's body
    /// fits.
    fn on(x: i32, z: i32, floor: &Floor) -> Self {
        Self {
            ox: floor.spot.0,
            oz: floor.spot.1,
            ..Self::at(x, z, floor.y)
        }
    }
    pub fn feet(self) -> Vec3 {
        Vec3::new(
            self.x as f32 * CELL + f32::from(self.ox) * SPOT_UNIT,
            self.y as f32 * 0.1,
            self.z as f32 * CELL + f32::from(self.oz) * SPOT_UNIT,
        )
    }
}
pub fn cell_of(p: Vec3) -> (i32, i32) {
    ((p.x / CELL).round() as i32, (p.z / CELL).round() as i32)
}

/// One step of a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Waypoint {
    pub feet: Vec3,
    /// Reaching it takes a jump.
    pub jump: bool,
    /// Reaching it takes going in through an opening: walk toward this
    /// point, past the opening on the near side, until carried to `feet`.
    pub through: Option<Vec3>,
    /// Only a crouched body fits there.
    pub crouch: bool,
    /// The leg of the route it belongs to: how the body gets there.
    pub mode: Mode,
}
impl Waypoint {
    /// A plain walk to `feet`.
    pub fn walk(feet: Vec3) -> Self {
        Self {
            feet,
            jump: false,
            through: None,
            crouch: false,
            mode: Mode::Walk,
        }
    }
}

/// How a body gets to a waypoint: the leg of the route it belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Mode {
    /// On foot: walking, stepping, jumping, crawling, through an opening.
    #[default]
    Walk,
    /// Swimming: its floor lies under water that floats the body.
    Swim,
    /// Jetting from the waypoint before (`from`, where it lifts off): up to
    /// `apex`, over at that height and down onto this one, measured to take
    /// `seconds`.
    Jet { from: Vec3, apex: f32, seconds: f32 },
    /// Leaping from the waypoint before (`from`, its takeoff) across open
    /// air onto this one, from a standstill, measured to take `seconds`
    /// ([`crate::reach::Leaps`]).
    Leap { from: Vec3, seconds: f32 },
}

/// The detour, in units walked, a route takes to keep the whole body off
/// what it would brush against (`Floor::snug`).
const SNUG: f32 = 0.6;
/// Most grid steps a pulled straight walk passes over at once.
const PULL_REACH: usize = 16;
/// How far past touching [`Ground::settle`] pushes a body out of what it
/// overlapped, so the standing box it then tests no longer touches it.
const SETTLE_GAP: f32 = 0.01;

/// A walk route off the grid with its corners pulled straight: from
/// `from`, each plain walking waypoint (no jump, crawl, opening or other
/// leg) heads for the farthest of the plain walk that follows which the
/// body walks straight to ([`Ground::walkable`]), so a diagonal is one line,
/// not a zig-zag of the grid's eight directions. Every waypoint of another
/// kind, the one before it (where that step starts) and the route's end
/// are kept; nothing is cut that the body would catch on. A chassis's
/// route is left as it is: its drive leg steers by pursuit, not by corners.
/// The least flat (x, z) distance between segments `a0`-`a1` and `b0`-`b1`.
fn segment_gap(a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3) -> f32 {
    let flat = |v: Vec3| glam::Vec2::new(v.x, v.z);
    let (a0, a1, b0, b1) = (flat(a0), flat(a1), flat(b0), flat(b1));
    let to_segment = |p: glam::Vec2, s0: glam::Vec2, s1: glam::Vec2| {
        let d = s1 - s0;
        let t = ((p - s0).dot(d) / d.length_squared().max(1e-9)).clamp(0.0, 1.0);
        (p - (s0 + d * t)).length()
    };
    let (da, db) = (a1 - a0, b1 - b0);
    let cross = |u: glam::Vec2, v: glam::Vec2| u.x * v.y - u.y * v.x;
    let denom = cross(da, db);
    if denom.abs() > 1e-9 {
        let t = cross(b0 - a0, db) / denom;
        let u = cross(b0 - a0, da) / denom;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
            return 0.0;
        }
    }
    to_segment(a0, b0, b1)
        .min(to_segment(a1, b0, b1))
        .min(to_segment(b0, a0, a1))
        .min(to_segment(b1, a0, a1))
}

pub fn pull(ground: &Ground, body: &Body, from: Vec3, path: Vec<Waypoint>) -> Vec<Waypoint> {
    if body.conservative {
        return path;
    }
    let plain = |w: &Waypoint| w.mode == Mode::Walk && !w.jump && !w.crouch && w.through.is_none();
    let mut pulled = Vec::with_capacity(path.len());
    let (mut anchor, mut i) = (from, 0);
    while i < path.len() {
        let mut keep = i;
        if plain(&path[i]) {
            let mut k = i + 1;
            while k < path.len()
                && k - i <= PULL_REACH
                && plain(&path[k])
                && ground.walkable(body, anchor, path[k].feet)
                && !ground.crowds(body, anchor, path[k].feet)
            {
                keep = k;
                // Where the next kind of step starts is kept.
                if path.get(k + 1).is_some_and(|w| !plain(w)) {
                    break;
                }
                k += 1;
            }
        }
        pulled.push(path[keep]);
        anchor = path[keep].feet;
        i = keep + 1;
    }
    pulled
}

/// A move of the grid: from one cell to another.
type Move = ((i32, i32), (i32, i32));

/// A move a body failed: from the floor at height `from`, onto the floor
/// at `to` (`None`: any, a way it got nowhere walking), avoided until.
#[derive(Clone, Copy, Debug)]
struct Failed {
    from: f32,
    to: Option<f32>,
    until: u64,
}

/// The eight cells round a cell.
const NEIGHBOURS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
];

/// A step of the grid: the node it reaches, whether that takes a jump, the
/// floor there, and the point it walks toward through an opening when the
/// step goes through one.
type Step = (Node, bool, Floor, Option<Vec3>);

/// The remembered ground samples.
#[derive(Default)]
pub struct Nav {
    /// The floors of a cell's column a body at a height hint reaches,
    /// highest first.
    floors: FxHashMap<(i32, i32, i32), Floors>,
    /// New samples left this tick.
    budget: u32,
    /// Node expansions left this tick.
    expansions: u32,
    /// Samples taken so far, for tests and probes.
    pub sampled: u64,
    /// Whether the arc of a leap from one node onto another is clear, as
    /// swept ([`Ground::sweep_arc`]), and how many more it sweeps this
    /// tick.
    arcs: FxHashMap<(Node, Node), bool>,
    arc_budget: u32,
    /// Arcs swept so far, for tests and probes.
    pub swept: u64,
    /// Floors found only off their cell's centre (settled), for probes.
    pub settled: u64,
    /// Moves, from one cell to another, a body was seen to fail (get
    /// nowhere walking, miss a leap), each from (and onto) which floor of
    /// those cells, with the tick until which the search takes no such
    /// move (`Nav::avoid`): what the samples miss (a rail between cell
    /// centres, a lip the motor catches on, a leap that clips), the grid
    /// learns from what really happened, for that move alone.
    avoid: FxHashMap<Move, Vec<Failed>>,
    /// The tick, for what it avoids.
    now: u64,
}
impl Nav {
    /// Start a tick's sampling budget.
    pub fn begin_tick(&mut self) {
        self.budget = SAMPLES_PER_TICK;
        self.expansions = EXPANSIONS_PER_TICK;
        self.arc_budget = ARCS_PER_TICK;
    }
    /// The tick it is: what it avoids lapses by it.
    pub fn set_now(&mut self, tick: u64) {
        self.now = tick;
        if self.avoid.len() > MAX_AVOIDED {
            self.avoid.retain(|_, failed| {
                failed.retain(|f| tick < f.until);
                !failed.is_empty()
            });
        }
    }
    /// A body failed the move from `from` to `to` (a leap that missed):
    /// the search takes no such move until `until`, so routes find another
    /// way, or none.
    pub fn avoid(&mut self, from: Vec3, to: Vec3, until: u64) {
        self.avoid
            .entry((cell_of(from), cell_of(to)))
            .or_default()
            .push(Failed {
                from: from.y,
                to: Some(to.y),
                until,
            });
    }
    /// A body standing at `feet` got nowhere walking toward `toward`: the
    /// steps out of its cell that way (within half a turn of the eight
    /// either side) are avoided until `until`.
    pub fn avoid_walk(&mut self, feet: Vec3, toward: Vec3, until: u64) {
        let from = cell_of(feet);
        let heading = Vec3::new(toward.x - feet.x, 0.0, toward.z - feet.z).normalize_or_zero();
        for (dx, dz) in NEIGHBOURS {
            let step = Vec3::new(dx as f32, 0.0, dz as f32).normalize();
            if heading != Vec3::ZERO && step.dot(heading) >= std::f32::consts::FRAC_1_SQRT_2 - 1e-3
            {
                self.avoid
                    .entry((from, (from.0 + dx, from.1 + dz)))
                    .or_default()
                    .push(Failed {
                        from: feet.y,
                        to: None,
                        until,
                    });
            }
        }
    }
    /// Whether the move from `from` onto the floor at `to` of cell `x, z`
    /// is avoided. Floors of one column are a crouched body's height apart
    /// at least: one within half that is the same floor.
    fn avoided(&self, from: Node, x: i32, z: i32, to: f32, body: &Body) -> bool {
        let same = |a: f32, b: f32| (a - b).abs() < body.crouch_height * 0.5;
        self.avoid
            .get(&((from.x, from.z), (x, z)))
            .is_some_and(|failed| {
                failed.iter().any(|f| {
                    self.now < f.until
                        && same(f.from, from.feet().y)
                        && f.to.is_none_or(|y| same(y, to))
                })
            })
    }
    pub fn clear(&mut self) {
        self.floors.clear();
        self.arcs.clear();
    }
    pub fn len(&self) -> usize {
        self.floors.len()
    }
    pub fn is_empty(&self) -> bool {
        self.floors.is_empty()
    }
    /// Forget samples a change inside `min..max` could alter.
    pub fn invalidate(&mut self, min: Vec3, max: Vec3, body: &Body) {
        let reach = body.width + CELL;
        let (x0, z0) = cell_of(min - Vec3::splat(reach));
        let (x1, z1) = cell_of(max + Vec3::splat(reach));
        // A sample's ray starts up to a jump plus a body above its hint.
        let low = min.y - body.height - body.jump - HINT_BAND * 2.0;
        let high = max.y + body.drop + HINT_BAND * 2.0;
        self.floors.retain(|(x, z, band), _| {
            let hint = *band as f32 * HINT_BAND;
            !((x0..=x1).contains(x) && (z0..=z1).contains(z) && hint >= low && hint <= high)
        });
        // An arc's box: from its takeoff to its landing, a body wide, up to
        // the top of its jump and a body over.
        let (min, max) = (min - Vec3::splat(reach), max + Vec3::splat(reach));
        let above = body.motion.apex() + body.height;
        self.arcs.retain(|(a, b), _| {
            let (a, b) = (a.feet(), b.feet());
            let (low, high) = (a.min(b), a.max(b) + Vec3::Y * above);
            !(low.cmple(max).all() && high.cmpge(min).all())
        });
    }
    /// Whether the arc of a leap from `from` onto `to` is clear
    /// ([`Ground::sweep_arc`]), remembered; `None` when this tick's arcs
    /// are spent.
    fn arc(&mut self, ground: &Ground, body: &Body, from: Node, to: Node) -> Option<bool> {
        if let Some(clear) = self.arcs.get(&(from, to)) {
            return Some(*clear);
        }
        if self.arc_budget == 0 {
            return None;
        }
        self.arc_budget -= 1;
        self.swept += 1;
        if self.arcs.len() >= MAX_SAMPLES {
            self.arcs.clear();
        }
        let clear = ground.sweep_arc(body, from.feet(), to.feet());
        self.arcs.insert((from, to), clear);
        Some(clear)
    }
    /// The floors of cell `x, z` a body standing at height `from` next to
    /// it could reach, highest first: none for a wall, a hole or a drop too
    /// deep. The ceiling over `from` limits how high it looks.
    fn floor(&mut self, ground: &Ground, body: &Body, x: i32, z: i32, from: f32) -> Option<Floors> {
        let band = (from / HINT_BAND).floor() as i32;
        if let Some(found) = self.floors.get(&(x, z, band)) {
            return Some(found.clone());
        }
        if self.budget == 0 {
            return None;
        }
        self.budget -= 1;
        self.sampled += 1;
        if self.floors.len() >= MAX_SAMPLES {
            self.floors.clear();
        }
        let found = sample(ground, body, x, z, band as f32 * HINT_BAND);
        self.settled += found.iter().filter(|f| f.spot != (0, 0)).count() as u64;
        self.floors.insert((x, z, band), found.clone());
        Some(found)
    }
    /// The node a body standing at `feet` occupies, if the ground there is
    /// walkable (`None` inside `Some` when it is not; `None` when out of
    /// budget).
    pub fn node_at(&mut self, ground: &Ground, body: &Body, feet: Vec3) -> Option<Option<Node>> {
        let (x, z) = cell_of(feet);
        let found = self.floor(ground, body, x, z, feet.y + body.step * 0.5)?;
        Some(nearest(&found, feet.y, body).map(|f| Node::on(x, z, &f)))
    }
    /// Walkable (or, when it `swims`, swimmable) neighbours of `node` and
    /// whether each takes a jump. `None` when the budget ran out before all
    /// eight were known.
    fn neighbours(
        &mut self,
        ground: &Ground,
        body: &Body,
        node: Node,
        swims: bool,
    ) -> Option<Vec<Step>> {
        let from = node.feet().y;
        // Afloat in deep water, the body rides high: it reaches the floors
        // of the same water whatever their depth, and climbs out onto a
        // bank from the surface.
        let afloat = ground
            .floats(body, node.feet())
            .filter(|_| swims)
            .map(|surface| (surface - body.height * 0.5).max(from));
        let mut straight = [None; 4];
        let mut out = Vec::with_capacity(8);
        const AXES: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        let middle = Vec3::Y * (body.height * 0.5);
        let goes_in = |dx: i32, dz: i32| {
            if body.conservative || ground.passages.list.is_empty() {
                return None;
            }
            let a = node.feet() + middle;
            let b = a + Vec3::new(dx as f32, 0.0, dz as f32) * CELL;
            ground
                .passages
                .first(a, b)
                .map(|(p, _)| (p.carry, b - middle))
        };
        for (i, (dx, dz)) in AXES.into_iter().enumerate() {
            if let Some((carry, past)) = goes_in(dx, dz) {
                // In through an opening: out by the partner, a little past
                // its plane so the cell is one in front of it.
                // The first cell it can stand in, within a body's width.
                let ahead = carry.transform_vector3(Vec3::new(dx as f32, 0.0, dz as f32));
                let out_at = carry.transform_point3(past + middle) - middle + ahead * 0.2;
                for k in 0..3 {
                    let at = out_at + ahead * (CELL * k as f32);
                    let (x, z) = cell_of(at);
                    let floors = self.floor(ground, body, x, z, at.y + body.step * 0.5)?;
                    if let Some(f) = nearest(&floors, at.y, body) {
                        let walk =
                            node.feet() + Vec3::new(dx as f32, 0.0, dz as f32) * (CELL * 3.0);
                        out.push((Node::on(x, z, &f), false, f, Some(walk)));
                        break;
                    }
                }
                continue;
            }
            let (x, z) = (node.x + dx, node.z + dz);
            let floors = self.floor(ground, body, x, z, from)?;
            let mut steps: Vec<(Floor, bool)> = entered(&floors, from, afloat, body)
                .filter_map(|f| Some((f, link(body, swims, from, afloat, f)?)))
                .collect();
            // Afloat, a bank too high to be found from the bottom is looked
            // for from the surface.
            if steps.is_empty()
                && let Some(level) = afloat.filter(|level| *level > from + body.step)
            {
                let floors = self.floor(ground, body, x, z, level)?;
                steps = floors
                    .iter()
                    .filter_map(|f| Some((*f, link(body, swims, from, afloat, *f)?)))
                    .collect();
            }
            for (f, jump) in steps {
                if self.avoided(node, x, z, f.y, body) {
                    continue;
                }
                let next = Node::on(x, z, &f);
                // A drop is stepped off only where the body clears the way
                // out over the edge at the height it stands at (crouched,
                // out of a crawlspace): a rail or a lip between the two
                // spots, which neither cell's own sample sees, holds it back.
                if f.y < from - body.step && {
                    let crouched = !ground.clear(body, node.feet(), false);
                    let (half_width, _, _) = body.clearance(crouched);
                    let height = if crouched {
                        body.crouch_height
                    } else {
                        body.height
                    };
                    !ground.sweep_box(half_width, height, node.feet(), next.feet().with_y(from))
                } {
                    continue;
                }
                // A jump is taken only where the body rises clear where it
                // stands to the height it lands at, then over onto it
                // (crouched, into a crawlspace): a roof over the takeoff, or
                // an overhang it would come up under, holds it back.
                if jump && afloat.is_none() && {
                    let (half_width, _, _) = body.clearance(f.low);
                    let height = if f.low {
                        body.crouch_height
                    } else {
                        body.height
                    };
                    let up = node.feet().with_y(f.y);
                    !(ground.sweep_box(half_width, height, node.feet(), up)
                        && ground.sweep_box(half_width, height, up, next.feet()))
                } {
                    continue;
                }
                // The way on level ground, for the diagonals beside it.
                if !jump && straight[i].is_none() {
                    straight[i] = Some((next, jump, f));
                }
                out.push((next, jump, f, None));
            }
        }
        // Diagonals only where both sides are open at walking height, so a
        // body never cuts a corner it would catch on.
        for (a, b) in [(0, 2), (0, 3), (1, 2), (1, 3)] {
            let (Some((na, false, fa)), Some((nb, false, fb))) = (straight[a], straight[b]) else {
                continue;
            };
            let (dx, dz) = (AXES[a].0, AXES[b].1);
            // Corners are never cut through an opening.
            if goes_in(dx, dz).is_some() {
                continue;
            }
            let floors = self.floor(ground, body, node.x + dx, node.z + dz, from)?;
            for f in entered(&floors, from, afloat, body) {
                if link(body, swims, from, afloat, f) == Some(false)
                    && !self.avoided(node, node.x + dx, node.z + dz, f.y, body)
                    && (f.wet && fa.wet && fb.wet
                        || (f.y - na.feet().y).abs() <= body.step
                            && (f.y - nb.feet().y).abs() <= body.step)
                {
                    out.push((Node::on(node.x + dx, node.z + dz, &f), false, f, None));
                }
            }
        }
        Some(out)
    }
}

/// Whether a body standing at height `from` (afloat at `afloat`) gets to a
/// neighbouring floor `to`: `Some(false)` walking or swimming, `Some(true)`
/// with a jump. A chassis never goes into deep water; a body that does not
/// take swim legs (`swims`) walks the bottom as if it were dry.
fn link(body: &Body, swims: bool, from: f32, afloat: Option<f32>, to: Floor) -> Option<bool> {
    if to.wet && !body.swims {
        return None;
    }
    if !swims {
        return edge(body, from, to);
    }
    if let Some(level) = afloat {
        // Within the same water it swims; out of it onto a bank level with
        // the bottom it swims out, onto a higher one it climbs from the
        // surface.
        return if to.wet || to.y - from <= body.step + 0.05 {
            Some(false)
        } else {
            (to.y - level <= body.jump).then_some(true)
        };
    }
    match edge(body, from, to) {
        // Deep water breaks a drop of any height.
        None if to.wet && to.y < from => Some(false),
        found => found,
    }
}

/// Whether a body at height `from` gets to a neighbouring floor at `to`:
/// `Some(false)` walking, `Some(true)` with a jump.
fn edge(body: &Body, from: f32, to: Floor) -> Option<bool> {
    let rise = to.y - from;
    // Into a crawlspace, a jump crouches in the air.
    let jump = if to.low { body.crawl_jump } else { body.jump };
    if rise < -body.drop {
        None
    } else if rise <= body.step + 0.05 {
        Some(false)
    } else if rise <= jump {
        Some(true)
    } else {
        None
    }
}

/// The floors of one column a body reaches, highest first.
type Floors = Vec<Floor>;

/// The floors of a column a body coming in at height `from` gets onto:
/// the one it walks or drops onto (the highest no more than a step above
/// its feet) and any higher, jumped onto. One under that is under a floor
/// it would come down on first. Afloat (`afloat`), it reaches them all.
fn entered<'a>(
    floors: &'a [Floor],
    from: f32,
    afloat: Option<f32>,
    body: &Body,
) -> impl Iterator<Item = Floor> + 'a {
    let reach = from + body.step + 0.05;
    let onto = floors
        .iter()
        .map(|f| f.y)
        .filter(|y| *y <= reach)
        .fold(f32::NEG_INFINITY, f32::max);
    floors
        .iter()
        .copied()
        .filter(move |f| afloat.is_some() || f.y >= onto)
}

/// The floor among `floors` a body standing at height `feet` is on: the
/// nearest within a step (and a little: the hint it was sampled from).
fn nearest(floors: &[Floor], feet: f32, body: &Body) -> Option<Floor> {
    floors
        .iter()
        .filter(|f| (f.y - feet).abs() <= body.step + 0.5)
        .min_by(|a, b| (a.y - feet).abs().total_cmp(&(b.y - feet).abs()))
        .copied()
}

/// A walkable floor.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Floor {
    y: f32,
    /// Where in its cell the body stands on it, in [`SPOT_UNIT`]s off the
    /// cell's centre: the centre unless only part of the cell holds it.
    spot: (i8, i8),
    /// The whole body touches something standing here: fine to pass,
    /// better avoided, so paths keep off walls and through the middle of
    /// doors.
    snug: bool,
    /// Only a crouched body fits.
    low: bool,
    /// Under water that floats the body: swum, not walked.
    wet: bool,
}

/// Every floor of cell `x, z` a body coming from height `from` reaches,
/// highest first, each where in the cell the body stands on it: upright
/// where it fits, else crouched (a crawlspace, walked in or jumped in to
/// crouching in the air, as high as it was measured to:
/// `Body::crawl_jump`). A column can hold several: a stair tread over the
/// floor beneath it, a roof over the crawlspace under it.
fn sample(ground: &Ground, body: &Body, x: i32, z: i32, from: f32) -> Floors {
    let (px, pz) = (x as f32 * CELL, z as f32 * CELL);
    // Nothing is reached above a jump, or above the ceiling over the cell
    // a body stands under at `from` (the roof of the room it is in): an
    // underside, facing down. A ray that starts inside a wall leaves it
    // through its top, which is no ceiling.
    let above = from + body.height;
    let ceiling = ground
        .ceiling(Vec3::new(px, above - 0.1, pz), body.jump + 0.2)
        .map_or(above + body.jump, |d| above - 0.1 + d);
    let reach = body.jump.max(body.crawl_jump);
    let mut top = (from + reach).min(ceiling - body.crouch_height) + 0.05;
    let bottom = from - body.drop - 0.1;
    let mut floors = Floors::new();
    for _ in 0..MAX_LAYERS {
        if top <= bottom {
            break;
        }
        let origin = Vec3::new(px, top, pz);
        let Some((distance, normal)) = ground.ray(origin, Vec3::NEG_Y, top - bottom) else {
            break;
        };
        let hit = top - distance;
        // A floor faces up and is not too steep; anything else (a steep
        // face) is passed, and so is where a ray that started inside
        // something (the plate it just stood on) leaves it underneath.
        if normal.y >= body.floor_cos && !ground.inside(origin) {
            let centre = Vec3::new(px, hit + 0.01, pz);
            // Upright, or else crouched; at the centre, or else wherever in
            // the cell the body fits.
            let placed = [false, true].into_iter().find_map(|crouched| {
                ground
                    .place(body, centre, crouched)
                    .map(|feet| (feet, crouched))
            });
            if let Some((feet, low)) = placed {
                let offset = |v: f32| (v / SPOT_UNIT).round() as i8;
                floors.push(Floor {
                    y: hit,
                    spot: (offset(feet.x - px), offset(feet.z - pz)),
                    snug: !ground.fits(body, feet, low),
                    low,
                    wet: ground.floats(body, feet).is_some(),
                });
            }
        }
        top = hit - 0.02;
    }
    floors
}

/// Surfaces one column's sample looks through, top to bottom.
const MAX_LAYERS: usize = 8;

#[derive(Clone, Copy, PartialEq)]
struct Open {
    f: f32,
    node: Node,
}
impl Eq for Open {}
impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f
            .total_cmp(&self.f)
            .then_with(|| other.node.cmp(&self.node))
    }
}
impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// How a search ended.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    /// A path to the goal.
    Path(Vec<Waypoint>),
    /// No path to the goal; a path to the reachable node closest to it.
    Partial(Vec<Waypoint>),
    /// Not even the start is walkable.
    Nowhere,
}

/// How the search reached a node: from where, by which edge, at what cost.
#[derive(Clone, Copy, Debug)]
struct Came {
    parent: Node,
    jump: bool,
    cost: f32,
    through: Option<Vec3>,
    crouch: bool,
    mode: Mode,
}

/// Most a leap's arc strays from the straight legs it is swept as: half a
/// cell, the grid's own resolution.
const ARC_SAG: f32 = CELL * 0.5;
/// Headings a body leaps along off an edge: the grid's eight and the ones
/// halfway between.
const LEAP_HEADINGS: u32 = 16;

/// Jet edges a search tests against the world (the shape sweeps) at most:
/// a cheap ray up rules out roofed cells first.
const MAX_JET_TESTS: u32 = 24;

/// An A* search from a body's feet to a goal, resumable across ticks: the
/// route planner's one search over every leg the body can take now
/// ([`Costs`]).
pub struct Search {
    pub goal: Vec3,
    start: Option<Node>,
    started: Vec3,
    open: BinaryHeap<Open>,
    came: FxHashMap<Node, Came>,
    best: Option<(Node, f32)>,
    expanded: u32,
    /// Nodes farther than this from the start (or from where an opening
    /// lets out), across, are not visited.
    bound: f32,
    /// The openings when the search began: where each is and where it
    /// lets out, with the estimate from there to the goal.
    links: Vec<(Vec3, Vec3, f32)>,
    /// The node being expanded when the budget ran out.
    pending: Option<Node>,
    costs: Costs,
    /// The floor at the goal a jet leg lands on, once sampled.
    landing: Option<Option<Node>>,
    /// Columns a takeoff was considered from, and sweeps spent.
    launches: FxHashSet<(i32, i32)>,
    jet_tests: u32,
    /// The body's walking speed, once known: the estimate's unit.
    walk_speed: f32,
}
impl Search {
    /// A search on foot and swimming, with no jets.
    pub fn new(from: Vec3, goal: Vec3, bound: f32) -> Self {
        Self::with(from, goal, bound, Costs::default())
    }
    /// A search over every leg `costs` allows.
    pub fn with(from: Vec3, goal: Vec3, bound: f32, costs: Costs) -> Self {
        Self {
            goal,
            start: None,
            started: from,
            open: BinaryHeap::new(),
            came: FxHashMap::default(),
            best: None,
            expanded: 0,
            bound,
            links: Vec::new(),
            pending: None,
            costs,
            landing: None,
            launches: FxHashSet::default(),
            jet_tests: 0,
            walk_speed: 1.0,
        }
    }
    /// Whether this search takes swim legs for `body`.
    fn swims(&self, body: &Body) -> bool {
        body.swims && self.costs.swim.is_some()
    }
    /// A lower bound on the distance from `a` to `b` a body covers.
    fn estimate(a: Vec3, b: Vec3) -> f32 {
        let d = a - b;
        Vec3::new(d.x, 0.0, d.z).length() + d.y.abs() * 0.5
    }
    /// The estimate to the goal, in seconds of walking: straight there, or
    /// to an opening and on from where it lets out, whichever is shorter.
    fn h(&self, node: Node) -> f32 {
        let feet = node.feet();
        self.links
            .iter()
            .map(|(entry, _, rest)| Self::estimate(feet, *entry) + rest)
            .fold(Self::estimate(feet, self.goal), f32::min)
            / self.walk_speed
    }
    fn arrived(&self, node: Node) -> bool {
        let d = node.feet() - self.goal;
        let swum = self.came.get(&node).is_some_and(|c| c.mode == Mode::Swim);
        Vec3::new(d.x, 0.0, d.z).length() <= ARRIVAL_RADIUS
            // Over a swum floor the goal may be anywhere up the water.
            && (d.y.abs() <= 2.0 || swum && d.y < 0.0)
            // Beside someone standing on the goal is as near as it gets.
            || self.landing == Some(Some(node))
    }
    /// Reach `to` the way `came` says, if that is cheaper than before.
    fn relax(&mut self, to: Node, came: Came) {
        if self.came.get(&to).is_some_and(|old| old.cost <= came.cost) {
            return;
        }
        self.came.insert(to, came);
        let h = self.h(to);
        if self.best.is_none_or(|(_, best)| h < best) {
            self.best = Some((to, h));
        }
        self.open.push(Open {
            f: came.cost + h,
            node: to,
        });
    }
    /// The node a body standing (or floating) at its start begins from:
    /// `None` inside when there is none, `None` when out of budget.
    fn first_node(&self, nav: &mut Nav, ground: &Ground, body: &Body) -> Option<Option<Node>> {
        if let Some(node) = nav.node_at(ground, body, self.started)? {
            return Some(Some(node));
        }
        // Afloat: its floor is under the water, out of reach of its feet.
        if self.swims(body) && ground.wet(body, self.started) {
            let top = self.started + Vec3::Y * 0.1;
            if let Some((distance, _)) = ground.ray(top, Vec3::NEG_Y, 64.0) {
                let bottom = Vec3::new(self.started.x, top.y - distance, self.started.z);
                if let Some(node) = nav.node_at(ground, body, bottom)? {
                    return Some(Some(node));
                }
            }
        }
        // Standing on an edge or a moving thing: try the cells around the
        // feet.
        let (x, z) = cell_of(self.started);
        for (dx, dz) in NEIGHBOURS {
            let floor = nav.floor(
                ground,
                body,
                x + dx,
                z + dz,
                self.started.y + body.step * 0.5,
            )?;
            if let Some(f) = nearest(&floor, self.started.y, body) {
                return Some(Some(Node::on(x + dx, z + dz, &f)));
            }
        }
        // Standing on something the grid leaves out (a vehicle's roof,
        // another body): the floor beneath, when it can drop down to it.
        let top = self.started + Vec3::Y * 0.1;
        if let Some((distance, _)) = ground.ray(top, Vec3::NEG_Y, body.drop + 0.1) {
            let bottom = Vec3::new(self.started.x, top.y - distance, self.started.z);
            if distance > body.step
                && let Some(node) = nav.node_at(ground, body, bottom)?
            {
                return Some(Some(node));
            }
        }
        Some(None)
    }
    /// A jet leg from `node` to the goal's floor, if the body's jets reach
    /// it and the air is clear: straight up where it stands, across at the
    /// crossing height, down onto the landing.
    fn jet_edge(&mut self, ground: &Ground, body: &Body, node: Node) -> Option<(Node, Came)> {
        let jets = self.costs.jets.as_ref()?;
        let landing = self.landing.flatten()?;
        let here = *self.came.get(&node)?;
        if landing == node || here.mode == Mode::Swim || !self.launches.insert((node.x, node.z)) {
            return None;
        }
        let (from, to) = (node.feet(), landing.feet());
        if to.y - from.y <= body.step + 0.05 {
            return None;
        }
        let apex = to.y + crate::route::JET_CLEARANCE;
        let seconds = jets.flight(from, to)?;
        // Straight up, the whole body rises where it lifts off, give or take
        // how near it stands to it. Open sky over its head and its corners
        // up to the crossing height: cheap rays rule out a roofed cell, or
        // one under an edge, before the sweeps.
        let half_width = body.width * 0.5 + crate::route::TAKEOFF_TOLERANCE;
        let head = from + Vec3::Y * body.height;
        let roofed = [
            (0.0, 0.0),
            (-1.0, -1.0),
            (-1.0, 1.0),
            (1.0, -1.0),
            (1.0, 1.0),
        ]
        .into_iter()
        .any(|(x, z)| {
            let at = head + Vec3::new(x, 0.0, z) * half_width;
            ground.ray(at, Vec3::Y, apex - from.y).is_some()
        });
        if self.jet_tests >= MAX_JET_TESTS || roofed {
            return None;
        }
        // Nor into someone standing over it.
        let (low, high) = (
            from + Vec3::new(-half_width, 0.1, -half_width),
            Vec3::new(from.x + half_width, apex + body.height, from.z + half_width),
        );
        if ground
            .bodies
            .iter()
            .any(|(min, max)| min.cmplt(high).all() && max.cmpgt(low).all())
        {
            return None;
        }
        self.jet_tests += 1;
        let top = Vec3::new(from.x, apex, from.z);
        let over = Vec3::new(to.x, apex, to.z);
        let clear = ground.sweep_box(half_width, body.height, from, top)
            && ground.sweep(body, top, over)
            && ground.sweep(body, over, to);
        clear.then(|| {
            (
                landing,
                Came {
                    parent: node,
                    jump: false,
                    cost: here.cost + jets.cost(seconds, from, to),
                    through: None,
                    crouch: false,
                    mode: Mode::Jet {
                        from,
                        apex,
                        seconds,
                    },
                },
            )
        })
    }
    /// Leaps from `node` ([`crate::reach::Leaps`]) off an edge: along each
    /// of [`LEAP_HEADINGS`] headings where the cell beside it is open air
    /// or a drop, onto the first support below and, past that, onto the
    /// far side (level, higher or lower). Onto each floor there it is
    /// measured to land on, where the whole arc is clear
    /// ([`Ground::sweep_arc`]). Each with the seconds it takes. `None`
    /// when the tick's samples or arcs ran out.
    fn leaps(
        &self,
        nav: &mut Nav,
        ground: &Ground,
        body: &Body,
        node: Node,
    ) -> Option<Vec<(Node, f32)>> {
        let mut out = Vec::new();
        let from = node.feet();
        let farthest = body.leaps.farthest();
        let swum = self.came.get(&node).is_none_or(|c| c.mode == Mode::Swim);
        if farthest <= 0.0 || swum || ground.floats(body, from).is_some() {
            return Some(out);
        }
        let level = |f: &Floor| !f.low && (f.y - from.y).abs() <= body.step + 0.05;
        for k in 0..LEAP_HEADINGS {
            let angle = k as f32 * std::f32::consts::TAU / LEAP_HEADINGS as f32;
            let heading = Vec3::new(angle.cos(), 0.0, angle.sin());
            // Over open air (off the edge beside it): the first support
            // below, once found.
            let (mut gap, mut support) = (false, None::<f32>);
            let mut last = (node.x, node.z);
            let mut across = CELL;
            while across <= farthest + CELL * 0.5 {
                let (x, z) = cell_of(from + heading * across);
                across += CELL;
                if (x, z) == last {
                    continue;
                }
                let beside = last == (node.x, node.z);
                last = (x, z);
                let floors = nav.floor(ground, body, x, z, from.y)?;
                let highest = floors.first().map(|f| f.y);
                let take = if !gap {
                    // Only off an edge: floor at its own level beside it is
                    // walked on, and a ledge or a wall there jumped onto
                    // from here or not at all.
                    if !beside
                        || floors.iter().any(level)
                        || highest.is_some_and(|h| h > from.y + body.step)
                    {
                        break;
                    }
                    gap = true;
                    highest.is_some()
                } else {
                    match (highest, support) {
                        (None, _) => false,
                        (Some(_), None) => true,
                        // The far side rising out of what the first
                        // support is the bottom of.
                        (Some(h), Some(s)) => h > s + body.step,
                    }
                };
                if !take {
                    continue;
                }
                for f in floors.iter().filter(|f| !f.wet && !f.low) {
                    if nav.avoided(node, x, z, f.y, body) {
                        continue;
                    }
                    let to = Node::on(x, z, f);
                    let feet = to.feet();
                    let d = feet - from;
                    let Some(seconds) = body.leaps.seconds(d.y, Vec3::new(d.x, 0.0, d.z).length())
                    else {
                        continue;
                    };
                    if nav.arc(ground, body, node, to)? {
                        out.push((to, seconds));
                    }
                }
                // Onto a ledge, the far side, or past the bottom of the
                // gap: nothing farther is leapt to.
                let h = highest.unwrap_or(f32::NEG_INFINITY);
                if support.is_some() || h >= from.y - body.step {
                    break;
                }
                support = Some(h);
            }
        }
        Some(out)
    }
    /// Search on until done or the tick's sampling budget is spent.
    pub fn step(&mut self, nav: &mut Nav, ground: &Ground, body: &Body) -> Option<Found> {
        // Where a jet leg would land: the goal's own floor, or, where
        // someone stands on it (a chased enemy), the nearest floor beside
        // them at that height, so it does not come down on their head. A
        // cell's leeway keeps the braking drift of the touchdown off them.
        if self.costs.jets.is_some() && self.landing.is_none() {
            let (x, z) = cell_of(self.goal);
            let (half_width, _, _) = body.clearance(false);
            let half_width = half_width + CELL;
            let taken = |feet: Vec3| {
                let (low, high) = (
                    feet + Vec3::new(-half_width, 0.1, -half_width),
                    feet + Vec3::new(half_width, body.height, half_width),
                );
                ground
                    .bodies
                    .iter()
                    .any(|(min, max)| min.cmplt(high).all() && max.cmpgt(low).all())
            };
            let mut landing = None;
            'rings: for ring in 0..=LANDING_RING {
                let mut best: Option<(f32, Node)> = None;
                for dx in -ring..=ring {
                    for dz in -ring..=ring {
                        if dx.abs().max(dz.abs()) != ring {
                            continue;
                        }
                        let floor =
                            nav.floor(ground, body, x + dx, z + dz, self.goal.y + body.step * 0.5)?;
                        let Some(f) = nearest(&floor, self.goal.y, body).filter(|f| !f.wet) else {
                            continue;
                        };
                        let node = Node::on(x + dx, z + dz, &f);
                        let off = (dx * dx + dz * dz) as f32;
                        if !taken(node.feet()) && best.is_none_or(|(b, _)| off < b) {
                            best = Some((off, node));
                        }
                    }
                }
                if let Some((_, node)) = best {
                    landing = Some(node);
                    break 'rings;
                }
                if ring == 0 && !taken(self.goal) {
                    // The goal's own cell has no floor: no landing.
                    break;
                }
            }
            self.landing = Some(landing);
        }
        let start = match self.start {
            Some(start) => start,
            None => {
                self.walk_speed = body.motion.forward.max(f32::EPSILON);
                let Some(node) = self.first_node(nav, ground, body)? else {
                    return Some(Found::Nowhere);
                };
                self.start = Some(node);
                self.links = ground
                    .passages
                    .list
                    .iter()
                    .map(|p| {
                        let exit = p.carry.transform_point3(p.centre);
                        (p.centre, exit, Self::estimate(exit, self.goal))
                    })
                    .collect();
                let swum = self.swims(body) && ground.floats(body, node.feet()).is_some();
                self.came.insert(
                    node,
                    Came {
                        parent: node,
                        jump: false,
                        cost: 0.0,
                        through: None,
                        crouch: false,
                        mode: if swum { Mode::Swim } else { Mode::Walk },
                    },
                );
                self.open.push(Open {
                    f: self.h(node),
                    node,
                });
                self.best = Some((node, self.h(node)));
                node
            }
        };
        loop {
            let node = match self.pending.take() {
                Some(node) => node,
                None => {
                    let Some(open) = self.open.pop() else {
                        return Some(self.finish(ground, body, false));
                    };
                    let g = self.came[&open.node].cost;
                    // A stale heap entry for a node since reached cheaper.
                    if open.f > g + self.h(open.node) + 1e-4 {
                        continue;
                    }
                    open.node
                }
            };
            if self.arrived(node) {
                self.best = Some((node, 0.0));
                return Some(self.finish(ground, body, true));
            }
            if self.expanded >= MAX_EXPANSIONS {
                return Some(self.finish(ground, body, false));
            }
            if nav.expansions == 0 {
                self.pending = Some(node);
                return None;
            }
            let swims = self.swims(body);
            let Some(next) = nav.neighbours(ground, body, node, swims) else {
                self.pending = Some(node);
                return None;
            };
            let Some(leaps) = self.leaps(nav, ground, body, node) else {
                self.pending = Some(node);
                return None;
            };
            nav.expansions -= 1;
            self.expanded += 1;
            let here = self.came[&node];
            for (to, jump, floor, through) in next {
                let within = |from: Vec3| {
                    let offset = to.feet() - from;
                    Vec3::new(offset.x, 0.0, offset.z).length() <= self.bound
                };
                if !within(start.feet()) && !self.links.iter().any(|(_, exit, _)| within(*exit)) {
                    continue;
                }
                let d = to.feet() - node.feet();
                let swim = floor.wet && swims;
                let flat_d = Vec3::new(d.x, 0.0, d.z).length();
                // Every move costs the seconds it takes the body. Through
                // an opening it is one step, wherever it lets out; a swim
                // goes at the swimmer's speed; a walk at the walk's, or the
                // crouched walk's into a crawlspace; a jump is in the air as
                // long as its hop; walking off a drop adds the fall.
                let seconds = match through {
                    Some(_) => body.seconds(CELL),
                    None if swim => {
                        let rate = self.costs.swim.unwrap_or_default();
                        body.seconds(
                            flat_d * rate.per_unit
                                + if here.mode == Mode::Swim {
                                    0.0
                                } else {
                                    rate.entry
                                },
                        )
                    }
                    None if jump => body
                        .motion
                        .hop(0.0, d.y)
                        .max(body.walk_seconds(flat_d, floor.low)),
                    None => {
                        body.walk_seconds(flat_d, floor.low) + body.motion.fall(-d.y - body.step)
                    }
                };
                let cost = here.cost
                    + seconds
                    // Keeps paths off walls, through the middle of doors.
                    + if floor.snug { body.seconds(SNUG) } else { 0.0 };
                self.relax(
                    to,
                    Came {
                        parent: node,
                        jump,
                        cost,
                        through,
                        crouch: floor.low,
                        mode: if swim { Mode::Swim } else { Mode::Walk },
                    },
                );
            }
            for (to, seconds) in leaps {
                let offset = to.feet() - start.feet();
                if Vec3::new(offset.x, 0.0, offset.z).length() > self.bound {
                    continue;
                }
                let d = to.feet() - node.feet();
                let flat_d = Vec3::new(d.x, 0.0, d.z).length();
                self.relax(
                    to,
                    Came {
                        parent: node,
                        jump: false,
                        // Stopping still to take off, then the leap; no
                        // quicker than walking there, so the estimate stays
                        // a lower bound.
                        cost: here.cost + (body.motion.stop() + seconds).max(body.seconds(flat_d)),
                        through: None,
                        crouch: false,
                        mode: Mode::Leap {
                            from: node.feet(),
                            seconds,
                        },
                    },
                );
            }
            if let Some((landing, came)) = self.jet_edge(ground, body, node) {
                self.relax(landing, came);
            }
        }
    }
    fn finish(&self, ground: &Ground, body: &Body, arrived: bool) -> Found {
        let Some((mut node, _)) = self.best else {
            return Found::Nowhere;
        };
        let mut steps = Vec::new();
        loop {
            let came = self.came[&node];
            // A walk node stands the whole body where it fits.
            let walks = came.mode == Mode::Walk && !came.jump && came.through.is_none();
            let feet = node.feet();
            steps.push(Waypoint {
                feet: walks
                    .then(|| ground.settle(body, feet, came.crouch))
                    .flatten()
                    .unwrap_or(feet),
                jump: came.jump,
                through: came.through,
                crouch: came.crouch,
                mode: came.mode,
            });
            if came.parent == node {
                break;
            }
            node = came.parent;
        }
        steps.reverse();
        // A leap takes off from where the waypoint before stands.
        for i in 1..steps.len() {
            if let Mode::Leap { seconds, .. } = steps[i].mode {
                steps[i].mode = Mode::Leap {
                    from: steps[i - 1].feet,
                    seconds,
                };
            }
        }
        let path = simplify(steps);
        if arrived {
            Found::Path(path)
        } else {
            Found::Partial(path)
        }
    }
}

/// Drop waypoints in the middle of straight, level, jump-free runs of one
/// leg that go through no opening.
fn simplify(steps: Vec<Waypoint>) -> Vec<Waypoint> {
    let mut out: Vec<Waypoint> = Vec::with_capacity(steps.len());
    for (i, step) in steps.iter().enumerate() {
        let special = |w: &Waypoint| {
            w.jump
                || w.through.is_some()
                || w.crouch
                || matches!(w.mode, Mode::Jet { .. } | Mode::Leap { .. })
        };
        let keep = i == 0 || i + 1 == steps.len() || special(step) || special(&steps[i + 1]) || {
            let a = step.feet - steps[i - 1].feet;
            let b = steps[i + 1].feet - step.feet;
            step.mode != steps[i - 1].mode
                || step.mode != steps[i + 1].mode
                || (a.x - b.x).abs() > 1e-3
                || (a.z - b.z).abs() > 1e-3
                || (a.y - b.y).abs() > 0.05
        };
        if keep {
            out.push(*step);
        }
    }
    // The first waypoint is where the body already stands.
    if out.len() > 1 {
        out.remove(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> Body {
        Body::of(&bri_motor::player::PlayerTuning::default(), 1.0)
    }
    fn world(boxes: &[(Vec3, Vec3)]) -> PhysicsWorld {
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
        physics
    }
    fn no_terrain(_: Vec3, _: Vec3, _: f32) -> Option<(f32, Vec3)> {
        None
    }
    static NO_PASSAGES: Passages = Passages {
        list: Vec::new(),
        closed: Vec::new(),
    };
    fn search(physics: &PhysicsWorld, from: Vec3, to: Vec3) -> (Found, Nav) {
        let ground = Ground {
            physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        let body = body();
        let mut nav = Nav::default();
        let mut search = Search::new(from, to, 60.0);
        for _ in 0..10_000 {
            nav.begin_tick();
            if let Some(found) = search.step(&mut nav, &ground, &body) {
                return (found, nav);
            }
        }
        panic!("search never finished");
    }
    fn floor() -> (Vec3, Vec3) {
        (Vec3::new(-40.0, -1.0, -40.0), Vec3::new(40.0, 0.0, 40.0))
    }
    fn path(found: Found) -> Vec<Waypoint> {
        match found {
            Found::Path(p) => p,
            other => panic!("no path: {other:?}"),
        }
    }

    #[test]
    fn a_chassis_uses_full_width_and_cannot_inherit_a_pedestrian_gap() {
        let physics = world(&[
            floor(),
            (Vec3::new(-4.0, 0.0, -2.0), Vec3::new(-1.4, 4.0, 2.0)),
            (Vec3::new(1.4, 0.0, -2.0), Vec3::new(4.0, 4.0, 2.0)),
        ]);
        let ground = Ground {
            physics: &physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        let mut nav = Nav::default();
        nav.begin_tick();
        assert!(nav.node_at(&ground, &body(), Vec3::ZERO).unwrap().is_some());
        let mut chassis = body();
        chassis.width = 3.0;
        chassis.conservative = true;
        chassis.bottom = 0.7;
        nav.clear();
        assert!(
            nav.node_at(&ground, &chassis, Vec3::ZERO)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_low_spawn_plate_under_the_hull_does_not_imprison_a_chassis() {
        let physics = world(&[
            floor(),
            (Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 0.2, 0.5)),
        ]);
        let ground = Ground {
            physics: &physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        let mut chassis = body();
        chassis.width = 4.0;
        chassis.height = 2.0;
        chassis.crouch_height = 2.0;
        chassis.step = 0.2;
        chassis.jump = 0.2;
        chassis.conservative = true;
        chassis.bottom = 0.8;
        let mut nav = Nav::default();
        let mut search = Search::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -8.0), 16.0);
        for _ in 0..100 {
            nav.begin_tick();
            if let Some(found) = search.step(&mut nav, &ground, &chassis) {
                let p = path(found);
                assert!(!p.is_empty(), "the chassis must actually leave the plate");
                assert!(
                    p.last().unwrap().feet.distance(Vec3::new(0.0, 0.0, -8.0)) < 1.0,
                    "the simplified route reaches its goal: {p:?}"
                );
                assert!(
                    p.iter()
                        .all(|p| !p.jump && !p.crouch && p.through.is_none())
                );
                // Hull clearance admits a low plate, never a real wall.
                let tall = world(&[
                    floor(),
                    (Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.5, 0.5)),
                ]);
                let blocked = Ground {
                    physics: &tall,
                    terrain: &no_terrain,
                    passages: &NO_PASSAGES,
                    waters: &[],
                    bodies: &[],
                    motions: &[],
                };
                nav.clear();
                nav.begin_tick();
                assert!(
                    nav.node_at(&blocked, &chassis, Vec3::ZERO)
                        .unwrap()
                        .is_none()
                );
                return;
            }
        }
        panic!("chassis route never completed");
    }

    #[test]
    fn open_ground_is_a_straight_line() {
        let physics = world(&[floor()]);
        let (found, _) = search(
            &physics,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
        );
        let p = path(found);
        assert_eq!(p.len(), 1, "{p:?}");
        assert!((p[0].feet - Vec3::new(10.0, 0.0, 0.0)).length() < 0.8);
    }

    #[test]
    fn walls_are_walked_around_through_the_door() {
        // A wall along x = 5 from z = -20 to 20, with a 2-unit door at z = 10.
        let physics = world(&[
            floor(),
            (Vec3::new(5.0, 0.0, -20.0), Vec3::new(5.5, 4.0, 9.0)),
            (Vec3::new(5.0, 0.0, 11.0), Vec3::new(5.5, 4.0, 20.0)),
        ]);
        let (found, _) = search(&physics, Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0));
        let p = path(found);
        // Where the path crosses the wall's line, it is in the door's middle.
        let mut at = Vec3::ZERO;
        let crossing = std::iter::once(at)
            .chain(p.iter().map(|w| w.feet))
            .collect::<Vec<_>>()
            .windows(2)
            .find_map(|s| {
                let (a, b) = (s[0], s[1]);
                (a.x < 5.25 && b.x >= 5.25).then(|| a.z + (b.z - a.z) * (5.25 - a.x) / (b.x - a.x))
            })
            .expect("crosses the wall");
        at.z = crossing;
        assert!(
            (at.z - 10.0).abs() <= 0.3,
            "through the door's middle: {crossing} {p:?}"
        );
        assert!(p.iter().all(|w| !w.jump));
    }

    #[test]
    fn a_low_ledge_is_stepped_and_a_tall_one_jumped() {
        let physics = world(&[
            floor(),
            (Vec3::new(3.0, 0.0, -2.0), Vec3::new(6.0, 0.6, 2.0)),
            (Vec3::new(6.0, 0.0, -2.0), Vec3::new(9.0, 2.6, 2.0)),
        ]);
        let (found, _) = search(&physics, Vec3::ZERO, Vec3::new(7.5, 2.6, 0.0));
        let p = path(found);
        let last = p.last().unwrap();
        assert!((last.feet.y - 2.6).abs() < 0.11, "{p:?}");
        assert_eq!(
            p.iter().filter(|w| w.jump).count(),
            1,
            "one jump, onto the tall block: {p:?}"
        );
    }

    #[test]
    fn a_wall_too_tall_to_jump_with_no_way_round_gives_a_partial_path() {
        let physics = world(&[
            floor(),
            (Vec3::new(5.0, 0.0, -40.0), Vec3::new(5.5, 8.0, 40.0)),
        ]);
        let (found, _) = search(&physics, Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0));
        match found {
            Found::Partial(p) => {
                let end = p.last().unwrap().feet;
                assert!(end.x < 5.0 && end.x > 3.5, "stops at the wall: {end:?}");
            }
            other => panic!("expected a partial path, got {other:?}"),
        }
    }

    #[test]
    fn a_low_ceiling_blocks_and_changes_are_forgotten_locally() {
        // A slab at knee height over a corridor: no way under it, even
        // crouched.
        let slab = (Vec3::new(4.0, 0.8, -40.0), Vec3::new(6.0, 4.0, 40.0));
        let physics = world(&[floor(), slab]);
        let (found, mut nav) = search(&physics, Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0));
        assert!(matches!(found, Found::Partial(_)), "{found:?}");
        let before = nav.len();
        // Removing the slab invalidates only samples near it.
        let physics = world(&[floor()]);
        nav.invalidate(slab.0, slab.1, &body());
        let kept = nav.len();
        assert!(kept < before && kept > 0, "{kept} of {before} kept");
        let ground = Ground {
            physics: &physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        let mut search = Search::new(Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), 60.0);
        let found = loop {
            nav.begin_tick();
            if let Some(found) = search.step(&mut nav, &ground, &body()) {
                break found;
            }
        };
        assert!(matches!(found, Found::Path(_)), "{found:?}");
    }

    /// A wall with a window up it too low to stand in: the only way on is
    /// a jump that crouches in the air, as high as the body was measured to
    /// get into one; set higher than that, there is no way.
    #[test]
    fn a_window_up_a_wall_is_jumped_into_crouching() {
        let wall = |sill: f32| {
            let top = sill + body().crouch_height + 0.3;
            world(&[
                floor(),
                (Vec3::new(4.0, 0.0, -40.0), Vec3::new(6.0, sill, 40.0)),
                (Vec3::new(4.0, top, -40.0), Vec3::new(6.0, 8.0, 40.0)),
            ])
        };
        let sill = body().crawl_jump - 0.3;
        assert!(sill > body().step + 0.5, "{sill}");

        let p = path(search(&wall(sill), Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0)).0);
        assert!(
            p.iter()
                .any(|w| w.jump && w.crouch && (w.feet.y - sill).abs() < 0.1),
            "jumped into the window crouching: {p:?}"
        );
        let (found, _) = search(
            &wall(body().crawl_jump + 0.5),
            Vec3::ZERO,
            Vec3::new(10.0, 0.0, 0.0),
        );
        assert!(matches!(found, Found::Partial(_)), "{found:?}");
    }

    /// Under a slab at head height a body fits only crouched: the path
    /// crawls under it, its waypoints there saying to crouch; with a door
    /// close by, it walks round upright instead.
    #[test]
    fn a_crawlspace_is_crawled_unless_a_door_is_near() {
        let slab = (Vec3::new(4.0, 1.6, -40.0), Vec3::new(6.0, 4.0, 40.0));
        let physics = world(&[floor(), slab]);
        let p = path(search(&physics, Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0)).0);
        let under = |w: &Waypoint| (4.0..=6.0).contains(&w.feet.x);
        assert!(p.iter().any(under), "under the slab: {p:?}");
        assert!(
            p.iter().filter(|w| under(w)).all(|w| w.crouch),
            "crouched under it: {p:?}"
        );
        assert!(
            p.iter().filter(|w| w.feet.x > 6.5).all(|w| !w.crouch),
            "upright again past it: {p:?}"
        );
        // A gap in the slab a step aside: walked through upright.
        let physics = world(&[
            floor(),
            (slab.0, Vec3::new(6.0, 4.0, 0.5)),
            (Vec3::new(4.0, 1.6, 2.5), slab.1),
        ]);
        let p = path(search(&physics, Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0)).0);
        assert!(p.iter().all(|w| !w.crouch), "walked the gap: {p:?}");
    }

    /// The search with a body of its own.
    fn search_as(physics: &PhysicsWorld, body: &Body, from: Vec3, to: Vec3) -> Found {
        let ground = Ground {
            physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        let mut nav = Nav::default();
        let mut search = Search::new(from, to, 60.0);
        loop {
            nav.begin_tick();
            if let Some(found) = search.step(&mut nav, &ground, body) {
                return found;
            }
        }
    }

    /// Every move costs the seconds it takes the body: crawling under a
    /// slab or walking round through a gap in it is chosen by the body's
    /// own crouched and upright speeds, with no rate set for crawling.
    #[test]
    fn a_crawl_or_a_walk_round_is_chosen_by_the_time_each_takes() {
        // A slab 2 across, low enough to crawl under, with a door 6 aside.
        let door = 6.0;
        let physics = world(&[
            floor(),
            (Vec3::new(4.0, 1.6, -40.0), Vec3::new(6.0, 4.0, door - 1.0)),
            (Vec3::new(4.0, 1.6, door + 1.0), Vec3::new(6.0, 4.0, 40.0)),
        ]);
        let crawls = |crouch_forward: f32| {
            let mut body = body();
            body.motion.crouch_forward = crouch_forward;
            let p = path(search_as(
                &physics,
                &body,
                Vec3::ZERO,
                Vec3::new(10.0, 0.0, 0.0),
            ));
            p.iter().any(|w| w.crouch)
        };
        let walk = body().motion.forward;
        // Crawling as fast as walking: straight under, never round.
        assert!(crawls(walk));
        // Crawling at a tenth of a walk: round through the door.
        assert!(!crawls(walk / 10.0));
        // The stock body: crawling 2 across at its crouched speed takes
        // less than the 12 more it walks to the door and back.
        assert!(crawls(body().motion.crouch_forward));
    }

    /// Floating plate treads with open risers, each overlapping the next:
    /// the spot a body fits on a tread, behind the overhang of the one
    /// above, is narrower than a cell and off its centre. The route stands
    /// the body there, and every waypoint is a place it fits.
    #[test]
    fn overlapping_open_treads_are_climbed_where_the_body_fits_on_each() {
        for (rise, run) in [
            (0.6, 0.5),
            (0.6, 0.75),
            (0.8, 0.5),
            (0.8, 0.75),
            (1.0, 0.75),
        ] {
            let depth = 1.0;
            let mut boxes = vec![floor()];
            for i in 0..6 {
                let y = rise * (i as f32 + 1.0);
                let x = 2.0 + run * i as f32;
                boxes.push((Vec3::new(x, y - 0.2, 0.0), Vec3::new(x + depth, y, 2.0)));
            }
            let top = rise * 7.0;
            let x = 2.0 + run * 6.0;
            boxes.push((Vec3::new(x, top - 0.2, 0.0), Vec3::new(x + 4.0, top, 2.0)));
            let physics = world(&boxes);
            let goal = Vec3::new(x + 2.0, top, 1.0);
            let p = path(search(&physics, Vec3::new(-1.0, 0.0, 1.0), goal).0);
            let ground = Ground {
                physics: &physics,
                terrain: &no_terrain,
                passages: &NO_PASSAGES,
                waters: &[],
                bodies: &[],
                motions: &[],
            };
            for w in &p {
                assert!(
                    ground.clear(&body(), w.feet + Vec3::Y * 0.01, w.crouch),
                    "rise {rise} run {run}: {w:?} stands in a tread"
                );
            }
        }
    }

    /// A body crouched under a roof too low to stand under, on whatever it
    /// stands on, always has a start: the floor under its feet, not the
    /// roof's top above it.
    #[test]
    fn a_body_under_a_low_roof_starts_from_where_it_crouches() {
        let body = body();
        for roof in [body.crouch_height + 0.3, body.height - 0.2] {
            let physics = world(&[
                floor(),
                (Vec3::new(-2.0, roof, -2.0), Vec3::new(2.0, roof + 0.2, 2.0)),
            ]);
            let p = path(search(&physics, Vec3::ZERO, Vec3::new(6.0, 0.0, 0.0)).0);
            assert!(
                p.iter().all(|w| w.feet.y < roof),
                "roof {roof}: out from under it, not over it: {p:?}"
            );
        }
    }

    /// A column holds every floor a body reaches in it: a thin roof over a
    /// crawlspace is two floors, the crawlspace (crouched) and the roof's
    /// top, as sampled from beside them.
    #[test]
    fn a_column_holds_every_floor_a_body_reaches() {
        let body = body();
        let roof = body.crouch_height + 0.3;
        let physics = world(&[
            floor(),
            (Vec3::new(2.0, roof, -2.0), Vec3::new(4.0, roof + 0.2, 2.0)),
        ]);
        let ground = Ground {
            physics: &physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        let (x, z) = cell_of(Vec3::new(3.0, 0.0, 0.0));
        let floors = sample(&ground, &body, x, z, 0.0);
        assert_eq!(floors.len(), 2, "{floors:?}");
        assert!(!floors[0].low && (floors[0].y - (roof + 0.2)).abs() < 0.01);
        assert!(floors[1].low && floors[1].y.abs() < 0.01);
    }

    #[test]
    fn searches_spend_at_most_the_tick_budget() {
        let physics = world(&[floor()]);
        let ground = Ground {
            physics: &physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        let body = body();
        let mut nav = Nav::default();
        let mut search = Search::new(Vec3::ZERO, Vec3::new(30.0, 0.0, 20.0), 60.0);
        let mut ticks = 0;
        loop {
            nav.begin_tick();
            let before = nav.sampled;
            let done = search.step(&mut nav, &ground, &body);
            assert!(nav.sampled - before <= u64::from(SAMPLES_PER_TICK));
            ticks += 1;
            if done.is_some() {
                break;
            }
        }
        assert!(ticks > 1, "a long path takes several ticks");
    }

    #[test]
    fn stairs_are_climbed_and_the_same_search_is_deterministic() {
        // Five 0.6-high steps, 1 unit deep, up to a landing.
        let mut boxes = vec![floor()];
        for i in 0..5 {
            let x = 3.0 + i as f32;
            boxes.push((
                Vec3::new(x, 0.0, -3.0),
                Vec3::new(x + 1.0, 0.6 * (i + 1) as f32, 3.0),
            ));
        }
        boxes.push((Vec3::new(8.0, 0.0, -3.0), Vec3::new(14.0, 3.0, 3.0)));
        let physics = world(&boxes);
        let a = path(search(&physics, Vec3::ZERO, Vec3::new(12.0, 3.0, 0.0)).0);
        let b = path(search(&physics, Vec3::ZERO, Vec3::new(12.0, 3.0, 0.0)).0);
        assert_eq!(a, b);
        assert!(a.iter().all(|w| !w.jump), "stairs need no jump: {a:?}");
        assert!((a.last().unwrap().feet.y - 3.0).abs() < 0.11);
    }

    #[test]
    fn a_jet_leg_lands_beside_someone_standing_on_its_goal() {
        // A 6-high platform with nothing to climb, and someone on it, in a
        // yard small enough for the search to spend its budget there.
        let physics = world(&[
            (Vec3::new(0.0, -1.0, -6.0), Vec3::new(20.0, 0.0, 6.0)),
            (Vec3::new(10.0, 0.0, -3.0), Vec3::new(15.0, 6.0, 3.0)),
        ]);
        let goal = Vec3::new(12.25, 6.0, 0.25);
        let t = bri_motor::player::PlayerTuning::default();
        let half = t.width * 0.5;
        let bodies = [(
            goal - Vec3::new(half, 0.0, half),
            goal + Vec3::new(half, t.stand_height, half),
        )];
        let ground = Ground {
            physics: &physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &bodies,
            motions: &[],
        };
        let costs = crate::route::Costs {
            jets: crate::route::Jets::of(&t, t.max_energy, 1.0),
            ..Default::default()
        };
        let mut nav = Nav::default();
        let mut search = Search::with(Vec3::new(4.0, 0.0, 0.25), goal, 60.0, costs);
        let found = loop {
            nav.begin_tick();
            if let Some(found) = search.step(&mut nav, &ground, &body()) {
                break found;
            }
        };
        let p = path(found);
        let landing = p
            .iter()
            .find(|w| matches!(w.mode, Mode::Jet { .. }))
            .expect("it jets up onto the platform");
        let (min, max) = bodies[0];
        let (width, _, _) = body().clearance(false);
        let clear = landing.feet.x + width <= min.x
            || landing.feet.x - width >= max.x
            || landing.feet.z + width <= min.z
            || landing.feet.z - width >= max.z;
        assert!(clear, "landed on their head: {landing:?} {p:?}");
        assert!((landing.feet.y - 6.0).abs() < 0.2, "{landing:?}");
        assert!(landing.feet.distance(goal) < 2.5, "{landing:?}");
    }

    #[test]
    fn a_portal_is_a_way_through_a_wall_with_no_way_round() {
        // A wall too tall to jump across the whole floor. An opening stands
        // in the open on the near side, facing the start, and lets out ten
        // units east, beyond the wall.
        let physics = world(&[
            floor(),
            (Vec3::new(5.0, 0.0, -40.0), Vec3::new(5.5, 8.0, 40.0)),
        ]);
        let carry = glam::Affine3A::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let passages = Passages {
            list: vec![bri_content::passage::Passage {
                brick: 1,
                centre: Vec3::new(2.25, 1.5, 0.0),
                normal: Vec3::NEG_X,
                u: Vec3::Z,
                v: Vec3::Y,
                half: glam::Vec2::new(1.0, 1.5),
                carry,
            }],
            closed: vec![],
        };
        let ground = Ground {
            physics: &physics,
            terrain: &no_terrain,
            passages: &passages,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        let goal = Vec3::new(16.0, 0.0, 3.0);
        let mut nav = Nav::default();
        let mut search = Search::new(Vec3::ZERO, goal, 60.0);
        let found = loop {
            nav.begin_tick();
            if let Some(found) = search.step(&mut nav, &ground, &body()) {
                break found;
            }
        };
        let p = path(found);
        let at = p
            .iter()
            .position(|w| w.through.is_some())
            .expect("the path goes through the opening");
        let through = p[at];
        // It walks into the opening from the near side and comes out by
        // the partner, past the wall.
        let walk = through.through.unwrap();
        assert!(walk.x > 2.25 && walk.x < 4.0 && walk.z.abs() < 1.0, "{p:?}");
        assert!(through.feet.x > 12.25 && through.feet.x < 13.5, "{p:?}");
        assert!(p[..at].iter().all(|w| w.feet.x < 2.25), "{p:?}");
        assert!((p.last().unwrap().feet - goal).length() < 1.0, "{p:?}");
    }

    fn open_ground(physics: &PhysicsWorld) -> Ground<'_> {
        Ground {
            physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &[],
            motions: &[],
        }
    }
    fn walked(from: Vec3, p: &[Waypoint]) -> f32 {
        std::iter::once(from)
            .chain(p.iter().map(|w| w.feet))
            .collect::<Vec<_>>()
            .windows(2)
            .map(|s| s[0].distance(s[1]))
            .sum()
    }
    /// Every leg of a pulled route is one the body walks straight.
    fn every_leg_walkable(ground: &Ground, from: Vec3, p: &[Waypoint]) {
        let mut at = from;
        for w in p {
            assert!(
                w.jump || ground.walkable(&body(), at, w.feet),
                "{at} -> {} is cut through something: {p:?}",
                w.feet
            );
            at = w.feet;
        }
    }

    #[test]
    fn a_pulled_diagonal_across_open_floor_is_about_a_straight_line() {
        let physics = world(&[floor()]);
        let ground = open_ground(&physics);
        for (from, goal) in [
            (Vec3::ZERO, Vec3::new(12.0, 0.0, 5.0)),
            (Vec3::new(-3.0, 0.0, 9.0), Vec3::new(14.0, 0.0, -2.0)),
        ] {
            let (found, _) = search(&physics, from, goal);
            let raw = path(found);
            let end = raw.last().unwrap().feet;
            assert!(end.distance(goal) < 1.0, "{raw:?}");
            let straight = from.distance(end);
            // The grid's eight directions zig-zag a diagonal.
            assert!(walked(from, &raw) > straight * 1.05, "{raw:?}");
            let pulled = pull(&ground, &body(), from, raw);
            assert!(
                walked(from, &pulled) <= straight * 1.05,
                "{} of {straight}: {pulled:?}",
                walked(from, &pulled)
            );
            every_leg_walkable(&ground, from, &pulled);
        }
    }

    #[test]
    fn a_pulled_route_still_takes_the_door_and_cuts_no_corner() {
        let physics = world(&[
            floor(),
            (Vec3::new(5.0, 0.0, -20.0), Vec3::new(5.5, 4.0, 9.0)),
            (Vec3::new(5.0, 0.0, 11.0), Vec3::new(5.5, 4.0, 20.0)),
            // A block whose corner a diagonal past it would clip.
            (Vec3::new(-6.0, 0.0, 3.0), Vec3::new(-2.0, 4.0, 7.0)),
        ]);
        let ground = open_ground(&physics);
        for (from, goal) in [
            (Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0)),
            (Vec3::new(-8.0, 0.0, 1.0), Vec3::new(10.0, 0.0, 12.0)),
            (Vec3::new(-4.0, 0.0, 0.0), Vec3::new(-4.0, 0.0, 10.0)),
        ] {
            let (found, _) = search(&physics, from, goal);
            let raw = path(found);
            let pulled = pull(&ground, &body(), from, raw.clone());
            assert!(pulled.len() <= raw.len());
            assert!((pulled.last().unwrap().feet - raw.last().unwrap().feet).length() < 1e-4);
            every_leg_walkable(&ground, from, &pulled);
            if goal.x > 5.25 {
                let crossing = std::iter::once(from)
                    .chain(pulled.iter().map(|w| w.feet))
                    .collect::<Vec<_>>()
                    .windows(2)
                    .find_map(|s| {
                        let (a, b) = (s[0], s[1]);
                        (a.x < 5.25 && b.x >= 5.25)
                            .then(|| a.z + (b.z - a.z) * (5.25 - a.x) / (b.x - a.x))
                    })
                    .expect("crosses the wall");
                // The whole body passes through the 2-unit door.
                assert!(
                    (crossing - 10.0).abs() <= 1.0 - body().width * 0.5,
                    "{crossing}: {pulled:?}"
                );
            }
        }
    }

    /// A doorway only a little wider than the body, off the cell grid: the
    /// route crosses where the whole body fits between the jambs.
    #[test]
    fn a_route_through_a_tight_doorway_keeps_the_whole_body_clear() {
        let tall = body().jump + body().height;
        let physics = world(&[
            floor(),
            (Vec3::new(8.0, 0.0, 22.0), Vec3::new(8.5, tall, 29.5)),
            (Vec3::new(8.0, 0.0, 31.0), Vec3::new(8.5, tall, 38.5)),
        ]);
        let ground = open_ground(&physics);
        let (from, goal) = (Vec3::new(16.0, 0.0, 30.0), Vec3::new(0.5, 0.0, 30.0));
        let (found, _) = search(&physics, from, goal);
        let raw = path(found);
        let pulled = pull(&ground, &body(), from, raw);
        every_leg_walkable(&ground, from, &pulled);
        let crossing = std::iter::once(from)
            .chain(pulled.iter().map(|w| w.feet))
            .collect::<Vec<_>>()
            .windows(2)
            .find_map(|s| {
                let (a, b) = (s[0], s[1]);
                (a.x > 8.25 && b.x <= 8.25).then(|| a.z + (b.z - a.z) * (8.25 - a.x) / (b.x - a.x))
            })
            .expect("crosses the wall");
        assert!(
            (crossing - 30.25).abs() <= 0.75 - body().width * 0.5,
            "{crossing}: {pulled:?}"
        );
    }

    #[test]
    fn pulling_keeps_every_jump_and_where_it_starts() {
        let physics = world(&[
            floor(),
            (Vec3::new(3.0, 0.0, -2.0), Vec3::new(6.0, 0.6, 2.0)),
            (Vec3::new(6.0, 0.0, -2.0), Vec3::new(9.0, 2.6, 2.0)),
        ]);
        let ground = open_ground(&physics);
        let from = Vec3::new(0.0, 0.0, -6.0);
        let (found, _) = search(&physics, from, Vec3::new(7.5, 2.6, 0.0));
        let raw = path(found);
        let pulled = pull(&ground, &body(), from, raw.clone());
        let jumps = |p: &[Waypoint]| {
            p.iter()
                .filter(|w| w.jump)
                .map(|w| w.feet)
                .collect::<Vec<_>>()
        };
        assert!(!jumps(&raw).is_empty(), "{raw:?}");
        assert_eq!(jumps(&raw), jumps(&pulled));
        for (i, w) in pulled.iter().enumerate().filter(|(_, w)| w.jump) {
            let before = raw.iter().position(|r| r.feet == w.feet).unwrap();
            if before > 0 {
                assert!(
                    i > 0 && pulled[i - 1].feet == raw[before - 1].feet,
                    "{pulled:?}"
                );
            }
        }
    }

    /// A column (min, max) `top` high over the void, `x0..x1` by `z0..z1`.
    fn column(x0: f32, x1: f32, z0: f32, z1: f32, top: f32) -> (Vec3, Vec3) {
        (Vec3::new(x0, -30.0, z0), Vec3::new(x1, top, z1))
    }
    fn leaps(p: &[Waypoint]) -> Vec<(Vec3, Vec3)> {
        p.iter()
            .filter_map(|w| match w.mode {
                Mode::Leap { from, .. } => Some((from, w.feet)),
                _ => None,
            })
            .collect()
    }

    /// Two roofs a gap apart over a fall no body walks off: level, a step
    /// down and a storey down. The route leaps it from the edge it walks
    /// to, onto the far roof; one wider than any leap lands is no route.
    #[test]
    fn a_gap_over_a_fall_is_leapt_from_the_edge() {
        let far = body().leaps.farthest();
        for rise in [0.0, -1.0, -2.5] {
            let gap = 2.0;
            let physics = world(&[
                column(-8.0, 0.0, -3.0, 3.0, 0.0),
                column(gap, gap + 8.0, -3.0, 3.0, rise),
            ]);
            let to = Vec3::new(gap + 3.0, rise, 0.0);
            let p = path(search(&physics, Vec3::new(-3.0, 0.0, 0.0), to).0);
            let leapt = leaps(&p);
            assert_eq!(leapt.len(), 1, "rise {rise}: {p:?}");
            let (from, onto) = leapt[0];
            assert!(from.x <= 0.0 && onto.x >= gap, "rise {rise}: {from} {onto}");
            assert!((onto.y - rise).abs() < 0.1, "rise {rise}: {onto}");
            // Too wide to land across.
            let wide = far + 2.0;
            let physics = world(&[
                column(-8.0, 0.0, -3.0, 3.0, 0.0),
                column(wide, wide + 8.0, -3.0, 3.0, rise),
            ]);
            let found = search(&physics, Vec3::new(-3.0, 0.0, 0.0), to + Vec3::X * wide).0;
            assert!(matches!(found, Found::Partial(_)), "rise {rise}: {found:?}");
        }
    }

    /// Brick pegs up a shaft over a fall, each higher than a jump onto the
    /// one after next and a gap from the last, up to a landing: climbed
    /// peg by peg, a leap each.
    #[test]
    fn pegs_over_a_fall_are_leapt_up_one_by_one() {
        let mut boxes = vec![column(-8.0, 0.0, -3.0, 3.0, 0.0)];
        let mut x = 0.0;
        for k in 1..=2 {
            x += 1.5;
            boxes.push(column(x, x + 0.5, -0.25, 0.25, k as f32 * 2.0));
            x += 0.5;
        }
        let landing = x + 1.5;
        boxes.push(column(landing, landing + 6.0, -3.0, 3.0, 6.0));
        let physics = world(&boxes);
        let to = Vec3::new(landing + 3.0, 6.0, 0.0);
        let p = path(search(&physics, Vec3::new(-3.0, 0.0, 0.0), to).0);
        assert_eq!(leaps(&p).len(), 3, "{p:?}");
    }

    /// A leap whose arc a roof cuts short is not taken.
    #[test]
    fn a_gap_under_a_low_roof_is_not_leapt() {
        let physics = world(&[
            column(-8.0, 0.0, -3.0, 3.0, 0.0),
            column(2.0, 10.0, -3.0, 3.0, 0.0),
            (Vec3::new(-8.0, 3.0, -3.0), Vec3::new(10.0, 4.0, 3.0)),
        ]);
        let found = search(
            &physics,
            Vec3::new(-3.0, 0.0, 0.0),
            Vec3::new(5.0, 0.0, 0.0),
        )
        .0;
        assert!(matches!(found, Found::Partial(_)), "{found:?}");
    }

    /// A corridor with a ledge on alternate walls, each a storey above the
    /// last, up to a deck (Close Quarters' ledge shafts): climbed ledge by
    /// ledge, and come back down the same way.
    #[test]
    fn a_ledge_shaft_is_climbed_and_come_back_down() {
        let rise = 2.4;
        let levels = 3;
        let top = rise * (levels as f32 + 1.0);
        let mut boxes = vec![
            floor(),
            (Vec3::new(-2.0, 0.0, -0.5), Vec3::new(5.0, top + 3.0, 0.0)),
            (Vec3::new(-2.0, 0.0, 3.0), Vec3::new(5.0, top + 3.0, 3.5)),
        ];
        for k in 0..levels {
            let y = rise * (k as f32 + 1.0);
            // Each a little farther out than the one two below, so a
            // jump off that one's edge clears it.
            let (x0, x1) = if k % 2 == 0 {
                (2.5 + k as f32 * 0.25, 3.5 + k as f32 * 0.25)
            } else {
                (-1.0, 0.0)
            };
            boxes.push((Vec3::new(x0, y - 0.2, 0.0), Vec3::new(x1, y, 3.0)));
        }
        boxes.push((Vec3::new(-2.0, top - 0.2, 0.0), Vec3::new(-0.5, top, 3.0)));
        let physics = world(&boxes);
        let (bottom, deck) = (Vec3::new(1.0, 0.0, 1.5), Vec3::new(-1.5, top, 1.5));
        let up = path(search(&physics, bottom, deck).0);
        assert!(leaps(&up).len() >= levels, "{up:?}");
        let down = path(search(&physics, deck, bottom).0);
        assert!(leaps(&down).len() >= levels - 1, "{down:?}");
    }

    /// A leap a body missed is avoided alone: the route leaps the gap
    /// another way.
    #[test]
    fn a_missed_leap_is_avoided_and_nothing_else() {
        let physics = world(&[
            column(-8.0, 0.0, -3.0, 3.0, 0.0),
            column(2.0, 10.0, -3.0, 3.0, 0.0),
        ]);
        let ground = Ground {
            physics: &physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        let (from, to) = (Vec3::new(-3.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0));
        let mut nav = Nav::default();
        let route = |nav: &mut Nav| {
            let mut search = Search::new(from, to, 60.0);
            loop {
                nav.begin_tick();
                if let Some(found) = search.step(nav, &ground, &body()) {
                    return path(found);
                }
            }
        };
        let first = leaps(&route(&mut nav))[0];
        nav.avoid(first.0, first.1, u64::MAX);
        let again = route(&mut nav);
        let second = leaps(&again)[0];
        assert!(
            cell_of(second.0) != cell_of(first.0) || cell_of(second.1) != cell_of(first.1),
            "{first:?} {second:?}"
        );
    }

    /// Standing on a roof over a room, a route into the room goes off the
    /// roof's edge and in under it, never down through the roof.
    #[test]
    fn a_floor_under_the_one_it_stands_on_is_reached_round_it() {
        let physics = world(&[
            floor(),
            (Vec3::new(0.0, 2.8, -4.0), Vec3::new(10.0, 3.0, 4.0)),
        ]);
        let p = path(search(&physics, Vec3::new(5.0, 3.0, 0.0), Vec3::new(5.0, 0.0, 0.0)).0);
        let mut at = Vec3::new(5.0, 3.0, 0.0);
        for w in &p {
            // Every way down is off the roof, outside it.
            if at.y > 2.0 && w.feet.y < 1.0 {
                let outside = |v: Vec3| v.x < 0.0 || v.x > 10.0 || v.z.abs() > 4.0;
                assert!(
                    outside(w.feet),
                    "down through the roof: {at} -> {}: {p:?}",
                    w.feet
                );
            }
            at = w.feet;
        }
    }

    /// A ledge over a lip it would come up under: no jump onto it from
    /// beneath the lip, only from where it rises clear.
    #[test]
    fn a_ledge_is_not_jumped_onto_from_under_its_lip() {
        // A block 3 high from x 2; its top overhangs back to x 0.5 as a
        // plate 2.8..3 over the floor before it (room to stand under),
        // from z -4 to 1.
        let physics = world(&[
            floor(),
            (Vec3::new(2.0, 0.0, -4.0), Vec3::new(8.0, 3.0, 4.0)),
            (Vec3::new(0.5, 2.8, -4.0), Vec3::new(2.0, 3.0, 1.0)),
        ]);
        let p = path(
            search(
                &physics,
                Vec3::new(-3.0, 0.0, -2.0),
                Vec3::new(5.0, 3.0, -2.0),
            )
            .0,
        );
        for (i, w) in p.iter().enumerate() {
            if w.jump {
                let before = if i == 0 {
                    Vec3::new(-3.0, 0.0, -2.0)
                } else {
                    p[i - 1].feet
                };
                assert!(
                    !(before.x > 0.5 && before.z > -4.0 && before.z < 1.0),
                    "jumped from under the lip at {before}: {p:?}"
                );
            }
        }
    }

    /// A failed move off a roof avoids that move from the roof alone: the
    /// same step from the floor under the roof stays open.
    #[test]
    fn a_failed_move_upstairs_leaves_the_one_below_open() {
        let physics = world(&[
            floor(),
            (Vec3::new(-4.0, 2.8, -4.0), Vec3::new(4.0, 3.0, 4.0)),
        ]);
        let ground = Ground {
            physics: &physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        let body = body();
        let mut nav = Nav::default();
        let (up, down) = (Vec3::new(0.0, 3.0, 0.0), Vec3::new(0.0, 0.0, 0.0));
        // Got nowhere walking +x on the roof.
        nav.avoid_walk(up, up + Vec3::X, u64::MAX);
        let steps = |nav: &mut Nav, feet: Vec3| {
            nav.begin_tick();
            let node = nav.node_at(&ground, &body, feet).unwrap().unwrap();
            nav.neighbours(&ground, &body, node, true).unwrap()
        };
        let east = |steps: Vec<Step>| steps.iter().any(|(n, ..)| n.x == 1 && n.z == 0);
        assert!(!east(steps(&mut nav, up)));
        assert!(east(steps(&mut nav, down)));
    }

    /// Leap arcs are swept within the tick's budget: a search over pegs
    /// spends no more each tick, and still finds the way.
    #[test]
    fn leap_arcs_spend_at_most_the_tick_budget() {
        let mut boxes = vec![column(-8.0, 0.0, -3.0, 3.0, 0.0)];
        for k in 1..=4 {
            let x = k as f32 * 2.0;
            boxes.push(column(x - 0.5, x, -0.25, 0.25, 0.0));
        }
        boxes.push(column(10.5, 16.0, -3.0, 3.0, 0.0));
        let physics = world(&boxes);
        let ground = Ground {
            physics: &physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
            waters: &[],
            bodies: &[],
            motions: &[],
        };
        let body = body();
        let mut nav = Nav::default();
        let mut search = Search::new(Vec3::new(-3.0, 0.0, 0.0), Vec3::new(13.0, 0.0, 0.0), 60.0);
        let found = loop {
            nav.begin_tick();
            let before = nav.swept;
            let done = search.step(&mut nav, &ground, &body);
            assert!(nav.swept - before <= u64::from(ARCS_PER_TICK));
            if let Some(found) = done {
                break found;
            }
        };
        assert!(!leaps(&path(found)).is_empty());
        assert!(nav.swept > 0);
    }
}
