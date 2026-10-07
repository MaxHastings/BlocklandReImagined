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
/// Cells either side of a place a body got nowhere walking into that the
/// grid avoids with it (`Nav::avoid`), and how many it keeps at most
/// before forgetting the lapsed ones.
const AVOID_REACH: i32 = 1;
const MAX_AVOIDED: usize = 4096;
/// Farthest (cells) a jet leg's landing moves off a goal someone stands on.
const LANDING_RING: i32 = 4;
/// Horizontal distance within which a grid node completes its search.
pub(crate) const ARRIVAL_RADIUS: f32 = CELL * 1.5;
/// New ground samples all bots together may take in one tick.
pub const SAMPLES_PER_TICK: u32 = 96;
/// Nodes all searches together expand in one tick, remembered ground or not.
pub const EXPANSIONS_PER_TICK: u32 = 384;
/// Nodes one search expands before it settles for the closest it reached.
pub const MAX_EXPANSIONS: u32 = 6000;
/// Remembered samples before the whole cache starts over.
const MAX_SAMPLES: usize = 1 << 18;
/// Height quantum of a sample's source hint.
const HINT_BAND: f32 = 0.25;

/// What the grid is sampled for: a standing player body and what its motor
/// can climb, jump and drop.
#[derive(Clone, Copy, Debug, PartialEq)]
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
        let shape = SharedShape::cuboid(body.width * 0.5, tall * 0.5, body.width * 0.5);
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

/// A grid node: a cell and the height of its floor, in tenths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Node {
    pub x: i32,
    pub z: i32,
    pub y: i32,
}
impl Node {
    fn at(x: i32, z: i32, feet: f32) -> Self {
        Self {
            x,
            z,
            y: (feet * 10.0).round() as i32,
        }
    }
    pub fn feet(self) -> Vec3 {
        Vec3::new(
            self.x as f32 * CELL,
            self.y as f32 * 0.1,
            self.z as f32 * CELL,
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

/// A step of the grid: the node it reaches, whether that takes a jump, the
/// floor there, and the point it walks toward through an opening when the
/// step goes through one.
type Step = (Node, bool, Floor, Option<Vec3>);

/// The remembered ground samples.
#[derive(Default)]
pub struct Nav {
    /// Floor under a cell below a height hint, or none.
    floors: FxHashMap<(i32, i32, i32), Option<Floor>>,
    /// New samples left this tick.
    budget: u32,
    /// Node expansions left this tick.
    expansions: u32,
    /// Samples taken so far, for tests and probes.
    pub sampled: u64,
    /// Cells a body was seen to get nowhere walking into, with the tick
    /// until which no step goes into them (`Nav::avoid`): what the samples
    /// miss (a rail between cell centres, a lip the motor catches on), the
    /// grid learns from what really happened.
    avoid: FxHashMap<(i32, i32), u64>,
    /// The tick, for what it avoids.
    now: u64,
}
impl Nav {
    /// Start a tick's sampling budget.
    pub fn begin_tick(&mut self) {
        self.budget = SAMPLES_PER_TICK;
        self.expansions = EXPANSIONS_PER_TICK;
    }
    /// The tick it is: what it avoids lapses by it.
    pub fn set_now(&mut self, tick: u64) {
        self.now = tick;
        if self.avoid.len() > MAX_AVOIDED {
            self.avoid.retain(|_, until| tick < *until);
        }
    }
    /// A body got nowhere walking toward `at`: no step goes into the cells
    /// round it until `until`, so routes find another way, or none.
    pub fn avoid(&mut self, at: Vec3, until: u64) {
        let (x, z) = cell_of(at);
        for dx in -AVOID_REACH..=AVOID_REACH {
            for dz in -AVOID_REACH..=AVOID_REACH {
                self.avoid.insert((x + dx, z + dz), until);
            }
        }
    }
    fn avoided(&self, x: i32, z: i32) -> bool {
        self.avoid
            .get(&(x, z))
            .is_some_and(|until| self.now < *until)
    }
    pub fn clear(&mut self) {
        self.floors.clear();
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
    }
    /// The floor under cell `x, z` a body standing at height `from` next to
    /// it could reach: `None` for a wall, a hole or a drop too deep. The
    /// ceiling over `from` limits how high it looks.
    fn floor(
        &mut self,
        ground: &Ground,
        body: &Body,
        x: i32,
        z: i32,
        from: f32,
    ) -> Option<Option<Floor>> {
        let band = (from / HINT_BAND).floor() as i32;
        if let Some(found) = self.floors.get(&(x, z, band)) {
            return Some(*found);
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
        self.floors.insert((x, z, band), found);
        Some(found)
    }
    /// The node a body standing at `feet` occupies, if the ground there is
    /// walkable (`None` inside `Some` when it is not; `None` when out of
    /// budget).
    pub fn node_at(&mut self, ground: &Ground, body: &Body, feet: Vec3) -> Option<Option<Node>> {
        let (x, z) = cell_of(feet);
        let found = self.floor(ground, body, x, z, feet.y + body.step * 0.5)?;
        Some(
            found
                .filter(|f| (f.y - feet.y).abs() <= body.step + 0.5)
                .map(|f| Node::at(x, z, f.y)),
        )
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
                    let floor = self.floor(ground, body, x, z, at.y + body.step * 0.5)?;
                    if let Some(f) = floor.filter(|f| (f.y - at.y).abs() <= body.step + 0.5) {
                        let walk =
                            node.feet() + Vec3::new(dx as f32, 0.0, dz as f32) * (CELL * 3.0);
                        out.push((Node::at(x, z, f.y), false, f, Some(walk)));
                        break;
                    }
                }
                continue;
            }
            let (x, z) = (node.x + dx, node.z + dz);
            if self.avoided(x, z) {
                continue;
            }
            let floor = self.floor(ground, body, x, z, from)?;
            let mut step = floor.and_then(|f| Some((f, link(body, swims, from, afloat, f)?)));
            // Afloat, a bank too high to be found from the bottom is looked
            // for from the surface.
            if step.is_none()
                && let Some(level) = afloat.filter(|level| *level > from + body.step)
            {
                let floor = self.floor(ground, body, x, z, level)?;
                step = floor.and_then(|f| Some((f, link(body, swims, from, afloat, f)?)));
            }
            // A drop is stepped off only where the body clears the way out
            // over the edge at the height it stands at (crouched, out of a
            // crawlspace): a rail or a lip between the two cells' centres,
            // which neither cell's own sample sees, holds it back.
            if let Some((f, jump)) = step
                && (f.y >= from - body.step || {
                    let crouched = !ground.clear(body, node.feet(), false);
                    let (half_width, _, _) = body.clearance(crouched);
                    let height = if crouched {
                        body.crouch_height
                    } else {
                        body.height
                    };
                    ground.sweep_box(
                        half_width,
                        height,
                        node.feet(),
                        Vec3::new(x as f32 * CELL, from, z as f32 * CELL),
                    )
                })
            {
                let next = Node::at(x, z, f.y);
                straight[i] = Some((next, jump, f));
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
            if goes_in(dx, dz).is_some() || self.avoided(node.x + dx, node.z + dz) {
                continue;
            }
            let floor = self.floor(ground, body, node.x + dx, node.z + dz, from)?;
            if let Some(f) = floor
                && link(body, swims, from, afloat, f) == Some(false)
                && (f.wet && fa.wet && fb.wet
                    || (f.y - na.feet().y).abs() <= body.step
                        && (f.y - nb.feet().y).abs() <= body.step)
            {
                out.push((Node::at(node.x + dx, node.z + dz, f.y), false, f, None));
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

/// A walkable floor.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Floor {
    y: f32,
    /// The whole body touches something standing here: fine to pass,
    /// better avoided, so paths keep off walls and through the middle of
    /// doors.
    snug: bool,
    /// Only a crouched body fits.
    low: bool,
    /// Under water that floats the body: swum, not walked.
    wet: bool,
}

/// Find the floor of cell `x, z` for a body coming from height `from`.
fn sample(ground: &Ground, body: &Body, x: i32, z: i32, from: f32) -> Option<Floor> {
    let (px, pz) = (x as f32 * CELL, z as f32 * CELL);
    // Nothing is reached above a jump, or above the ceiling over the cell
    // a body stands under at `from` (the roof of the room it is in).
    let above = from + body.height;
    let ceiling = ground
        .ray(Vec3::new(px, above - 0.1, pz), Vec3::Y, body.jump + 0.2)
        .map_or(above + body.jump, |(d, _)| above - 0.1 + d);
    let mut top = (from + body.jump).min(ceiling - body.height) + 0.05;
    let bottom = from - body.drop - 0.1;
    for _ in 0..8 {
        if top <= bottom {
            break;
        }
        let origin = Vec3::new(px, top, pz);
        let Some((distance, normal)) = ground.ray(origin, Vec3::NEG_Y, top - bottom) else {
            break;
        };
        let hit = top - distance;
        // A floor faces up and is not too steep; anything else (the
        // underside of what the ray started in, a steep face) is passed.
        let feet = Vec3::new(px, hit + 0.01, pz);
        if normal.y >= body.floor_cos && ground.clear(body, feet, false) {
            return Some(Floor {
                y: hit,
                snug: !ground.fits(body, feet, false),
                low: false,
                wet: ground.floats(body, feet).is_some(),
            });
        }
        top = hit - 0.02;
    }
    crawl(ground, body, px, pz, from)
}

/// A floor at cell `px, pz` a body fits only crouched, from height `from`:
/// walked in to, or jumped in to crouching in the air (a window up a wall),
/// as high as it was measured to (`Body::crawl_jump`).
fn crawl(ground: &Ground, body: &Body, px: f32, pz: f32, from: f32) -> Option<Floor> {
    let mut top = from + body.crawl_jump.max(body.step) + 0.05;
    let bottom = from - body.drop - 0.1;
    // Past the undersides and walls the ray starts in or meets on the way
    // down, as `sample` does.
    for _ in 0..8 {
        if top <= bottom {
            break;
        }
        let (distance, normal) = ground.ray(Vec3::new(px, top, pz), Vec3::NEG_Y, top - bottom)?;
        let hit = top - distance;
        let feet = Vec3::new(px, hit + 0.01, pz);
        if normal.y >= body.floor_cos && ground.clear(body, feet, true) {
            return Some(Floor {
                y: hit,
                snug: !ground.fits(body, feet, true),
                low: true,
                wet: ground.floats(body, feet).is_some(),
            });
        }
        top = hit - 0.02;
    }
    None
}

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
        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
        ] {
            let floor = nav.floor(
                ground,
                body,
                x + dx,
                z + dz,
                self.started.y + body.step * 0.5,
            )?;
            if let Some(f) = floor.filter(|f| (f.y - self.started.y).abs() <= body.step + 0.5) {
                return Some(Some(Node::at(x + dx, z + dz, f.y)));
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
                        let Some(f) = floor
                            .filter(|f| (f.y - self.goal.y).abs() <= body.step + 0.5 && !f.wet)
                        else {
                            continue;
                        };
                        let node = Node::at(x + dx, z + dz, f.y);
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
            w.jump || w.through.is_some() || w.crouch || matches!(w.mode, Mode::Jet { .. })
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
}
