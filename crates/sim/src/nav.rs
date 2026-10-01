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
//! The openings of linked bricks (portals) are links in the grid: a step
//! whose body middle goes in through one, as the motor carries a body, leads
//! to the cell it comes out at by the partner, so paths lead through portals
//! wherever walking through one is the way.
use bri_content::passage::Passages;
use glam::Vec3;
use rapier3d::prelude::*;
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Grid spacing, in world units: one brick stud.
pub const CELL: f32 = 0.5;
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
    /// Highest ledge it walks up without jumping.
    pub step: f32,
    /// Highest ledge a jump lands it on.
    pub jump: f32,
    /// Deepest drop it walks off.
    pub drop: f32,
    /// Cosine of the steepest floor it stands on.
    pub floor_cos: f32,
}
impl Body {
    pub fn of(tuning: &bri_motor::player::PlayerTuning, scale: f32) -> Self {
        let rise = tuning.jump_speed * tuning.jump_speed / (2.0 * tuning.gravity.max(1.0));
        Self {
            width: tuning.width * scale,
            height: tuning.stand_height * scale,
            step: tuning.step_height,
            // Leave room for the takeoff: the apex is only brushed.
            jump: (rise * 0.8).max(tuning.step_height),
            drop: 4.0,
            floor_cos: tuning.slope_degrees.to_radians().cos(),
        }
    }
    /// The box a clearance test uses: narrower by one cell so a body that
    /// fits a gap passes whichever cell centre it is aligned to, and without
    /// the bottom half step, which the motor steps over.
    fn clearance(&self) -> (f32, f32, f32) {
        let half_width = ((self.width - CELL) * 0.5).max(self.width * 0.25);
        let lift = (self.step * 0.5).min(self.height * 0.3);
        (half_width, lift, self.height - lift)
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
}
impl Ground<'_> {
    fn filter() -> QueryFilter<'static> {
        QueryFilter::only_fixed().exclude_sensors()
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
    /// Whether a body stands at `feet` without touching fixed collision,
    /// allowing for grid alignment.
    fn clear(&self, body: &Body, feet: Vec3) -> bool {
        let (half_width, lift, tall) = body.clearance();
        self.empty(feet, half_width, lift, tall)
    }
    /// Whether the full-width body stands at `feet` touching nothing.
    fn fits(&self, body: &Body, feet: Vec3) -> bool {
        let (_, lift, tall) = body.clearance();
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
}

/// A step of the grid: the node it reaches, whether that takes a jump,
/// whether the body is snug there, and the point it walks toward through
/// an opening when the step goes through one.
type Step = (Node, bool, bool, Option<Vec3>);

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
}
impl Nav {
    /// Start a tick's sampling budget.
    pub fn begin_tick(&mut self) {
        self.budget = SAMPLES_PER_TICK;
        self.expansions = EXPANSIONS_PER_TICK;
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
    /// Walkable neighbours of `node` and whether each takes a jump.
    /// `None` when the budget ran out before all eight were known.
    fn neighbours(&mut self, ground: &Ground, body: &Body, node: Node) -> Option<Vec<Step>> {
        let from = node.feet().y;
        let mut straight = [None; 4];
        let mut out = Vec::with_capacity(8);
        const AXES: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        let middle = Vec3::Y * (body.height * 0.5);
        let goes_in = |dx: i32, dz: i32| {
            if ground.passages.list.is_empty() {
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
                        out.push((Node::at(x, z, f.y), false, f.snug, Some(walk)));
                        break;
                    }
                }
                continue;
            }
            let floor = self.floor(ground, body, node.x + dx, node.z + dz, from)?;
            if let Some(f) = floor
                && let Some(jump) = edge(body, from, f.y)
            {
                let next = Node::at(node.x + dx, node.z + dz, f.y);
                straight[i] = Some((next, jump));
                out.push((next, jump, f.snug, None));
            }
        }
        // Diagonals only where both sides are open at walking height, so a
        // body never cuts a corner it would catch on.
        for (a, b) in [(0, 2), (0, 3), (1, 2), (1, 3)] {
            let (Some((na, false)), Some((nb, false))) = (straight[a], straight[b]) else {
                continue;
            };
            let (dx, dz) = (AXES[a].0, AXES[b].1);
            // Corners are never cut through an opening.
            if goes_in(dx, dz).is_some() {
                continue;
            }
            let floor = self.floor(ground, body, node.x + dx, node.z + dz, from)?;
            if let Some(f) = floor
                && edge(body, from, f.y) == Some(false)
                && (f.y - na.feet().y).abs() <= body.step
                && (f.y - nb.feet().y).abs() <= body.step
            {
                out.push((Node::at(node.x + dx, node.z + dz, f.y), false, f.snug, None));
            }
        }
        Some(out)
    }
}

/// Whether a body at height `from` gets to a neighbouring floor at `to`:
/// `Some(false)` walking, `Some(true)` with a jump.
fn edge(body: &Body, from: f32, to: f32) -> Option<bool> {
    let rise = to - from;
    if rise < -body.drop {
        None
    } else if rise <= body.step + 0.05 {
        Some(false)
    } else if rise <= body.jump {
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
            return None;
        }
        let origin = Vec3::new(px, top, pz);
        let (distance, normal) = ground.ray(origin, Vec3::NEG_Y, top - bottom)?;
        let hit = top - distance;
        // A floor faces up and is not too steep; anything else (the
        // underside of what the ray started in, a steep face) is passed.
        let feet = Vec3::new(px, hit + 0.01, pz);
        if normal.y >= body.floor_cos && ground.clear(body, feet) {
            return Some(Floor {
                y: hit,
                snug: !ground.fits(body, feet),
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

/// An A* search from a body's feet to a goal, resumable across ticks.
pub struct Search {
    pub goal: Vec3,
    start: Option<Node>,
    started: Vec3,
    open: BinaryHeap<Open>,
    came: FxHashMap<Node, (Node, bool, f32, Option<Vec3>)>,
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
}
impl Search {
    pub fn new(from: Vec3, goal: Vec3, bound: f32) -> Self {
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
        }
    }
    fn estimate(a: Vec3, b: Vec3) -> f32 {
        let d = a - b;
        Vec3::new(d.x, 0.0, d.z).length() + d.y.abs() * 0.5
    }
    /// The estimate to the goal: straight there, or to an opening and on
    /// from where it lets out, whichever is shorter.
    fn h(&self, node: Node) -> f32 {
        let feet = node.feet();
        self.links
            .iter()
            .map(|(entry, _, rest)| Self::estimate(feet, *entry) + rest)
            .fold(Self::estimate(feet, self.goal), f32::min)
    }
    fn arrived(&self, node: Node) -> bool {
        let d = node.feet() - self.goal;
        Vec3::new(d.x, 0.0, d.z).length() <= CELL * 1.5 && d.y.abs() <= 2.0
    }
    /// Search on until done or the tick's sampling budget is spent.
    pub fn step(&mut self, nav: &mut Nav, ground: &Ground, body: &Body) -> Option<Found> {
        let start = match self.start {
            Some(start) => start,
            None => {
                let node = match nav.node_at(ground, body, self.started)? {
                    Some(node) => node,
                    None => {
                        // Standing on an edge or a moving thing: try the
                        // cells around the feet.
                        let (x, z) = cell_of(self.started);
                        let mut near = None;
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
                            if let Some(f) =
                                floor.filter(|f| (f.y - self.started.y).abs() <= body.step + 0.5)
                            {
                                near = Some(Node::at(x + dx, z + dz, f.y));
                                break;
                            }
                        }
                        match near {
                            Some(node) => node,
                            None => return Some(Found::Nowhere),
                        }
                    }
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
                self.came.insert(node, (node, false, 0.0, None));
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
                        return Some(self.finish(false));
                    };
                    let g = self.came[&open.node].2;
                    // A stale heap entry for a node since reached cheaper.
                    if open.f > g + self.h(open.node) + 1e-4 {
                        continue;
                    }
                    open.node
                }
            };
            if self.arrived(node) {
                self.best = Some((node, 0.0));
                return Some(self.finish(true));
            }
            if self.expanded >= MAX_EXPANSIONS {
                return Some(self.finish(false));
            }
            if nav.expansions == 0 {
                self.pending = Some(node);
                return None;
            }
            let Some(next) = nav.neighbours(ground, body, node) else {
                self.pending = Some(node);
                return None;
            };
            nav.expansions -= 1;
            self.expanded += 1;
            let g = self.came[&node].2;
            for (to, jump, snug, through) in next {
                let within = |from: Vec3| {
                    let offset = to.feet() - from;
                    Vec3::new(offset.x, 0.0, offset.z).length() <= self.bound
                };
                if !within(start.feet()) && !self.links.iter().any(|(_, exit, _)| within(*exit)) {
                    continue;
                }
                let d = to.feet() - node.feet();
                // Through an opening it is one step, wherever it lets out.
                let (across, drop) = match through {
                    Some(_) => (CELL, 0.0),
                    None => (Vec3::new(d.x, 0.0, d.z).length(), (-d.y).max(0.0)),
                };
                let cost = g
                    + across
                    + if jump { 1.0 } else { 0.0 }
                    + if snug { 0.6 } else { 0.0 }
                    + drop * 0.1;
                if self
                    .came
                    .get(&to)
                    .is_some_and(|(_, _, old, _)| *old <= cost)
                {
                    continue;
                }
                self.came.insert(to, (node, jump, cost, through));
                let h = self.h(to);
                if self.best.is_none_or(|(_, best)| h < best) {
                    self.best = Some((to, h));
                }
                self.open.push(Open {
                    f: cost + h,
                    node: to,
                });
            }
        }
    }
    fn finish(&self, arrived: bool) -> Found {
        let Some((mut node, _)) = self.best else {
            return Found::Nowhere;
        };
        let mut steps = Vec::new();
        loop {
            let (parent, jump, _, through) = self.came[&node];
            steps.push(Waypoint {
                feet: node.feet(),
                jump,
                through,
            });
            if parent == node {
                break;
            }
            node = parent;
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

/// Drop waypoints in the middle of straight, level, jump-free runs that go
/// through no opening.
fn simplify(steps: Vec<Waypoint>) -> Vec<Waypoint> {
    let mut out: Vec<Waypoint> = Vec::with_capacity(steps.len());
    for (i, step) in steps.iter().enumerate() {
        let special = |w: &Waypoint| w.jump || w.through.is_some();
        let keep = i == 0 || i + 1 == steps.len() || special(step) || special(&steps[i + 1]) || {
            let a = step.feet - steps[i - 1].feet;
            let b = steps[i + 1].feet - step.feet;
            (a.x - b.x).abs() > 1e-3 || (a.z - b.z).abs() > 1e-3 || (a.y - b.y).abs() > 0.05
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
        // A slab at head height over a corridor: no way under it.
        let slab = (Vec3::new(4.0, 1.6, -40.0), Vec3::new(6.0, 4.0, 40.0));
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

    #[test]
    fn searches_spend_at_most_the_tick_budget() {
        let physics = world(&[floor()]);
        let ground = Ground {
            physics: &physics,
            terrain: &no_terrain,
            passages: &NO_PASSAGES,
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
}
